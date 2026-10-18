//! SNCF connector for autocruise.
//!
//! Uses the official SNCF API (Navitia): <https://numerique.sncf.com/startup/api/>.
//! Read-only: stations, journeys, departures/arrivals with real-time delays,
//! train details and disruptions. **No prices and no booking**: the API does
//! not provide them.
//!
//! Times: the API speaks Europe/Paris local time without offset. This crate
//! sends request times as UTC (`...Z`) and returns RFC 3339 times in
//! Europe/Paris with an explicit offset.

pub mod client;
mod convert;
mod error;
mod navitia;
pub mod types;

use autocruise_core::time;

pub use client::SncfClient;
pub use error::SncfError;
use types::*;

const ID_PREFIXES: &[&str] = &["stop_area:", "stop_point:", "admin:", "address:", "poi:"];

/// The SNCF connector. Cheap to share behind an `Arc`.
#[derive(Debug)]
pub struct Sncf {
    client: SncfClient,
}

impl Sncf {
    pub fn new(client: SncfClient) -> Self {
        Self { client }
    }

    pub fn from_env() -> Self {
        Self::new(SncfClient::from_env())
    }

    pub async fn search_places(
        &self,
        p: SearchPlacesParams,
    ) -> Result<SearchPlacesResult, SncfError> {
        let query = p.query.trim();
        if query.is_empty() {
            return Err(SncfError::Invalid("query must not be empty".into()));
        }
        let kinds = p
            .kinds
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| vec![PlaceKind::Station, PlaceKind::City]);
        let count = clamp(p.count, 5, 20);
        let mut q = vec![("q", query.to_owned()), ("count", count.to_string())];
        q.extend(
            kinds
                .iter()
                .map(|k| ("type[]", convert::navitia_type(*k).to_owned())),
        );
        let res: navitia::PlacesResponse = self
            .client
            .get("places", &q, true)
            .await
            .or_else(empty_if_no_result)?;
        Ok(SearchPlacesResult {
            places: res.places.iter().filter_map(convert::place).collect(),
        })
    }

    pub async fn nearby_stations(
        &self,
        p: NearbyStationsParams,
    ) -> Result<NearbyStationsResult, SncfError> {
        check_coords(p.latitude, p.longitude)?;
        let radius = p.radius_m.unwrap_or(1000).clamp(50, 5000);
        let count = clamp(p.count, 5, 20);
        let path = format!("coord/{};{}/places_nearby", p.longitude, p.latitude);
        let q = [
            ("type[]", "stop_area".to_owned()),
            ("distance", radius.to_string()),
            ("count", count.to_string()),
        ];
        let res: navitia::PlacesResponse = self
            .client
            .get(&path, &q, true)
            .await
            .or_else(empty_if_no_result)?;
        Ok(NearbyStationsResult {
            stations: res
                .places_nearby
                .iter()
                .filter_map(convert::place)
                .collect(),
        })
    }

    pub async fn plan_journey(&self, p: PlanJourneyParams) -> Result<PlanJourneyResult, SncfError> {
        let from = self.resolve(&p.from, true).await?;
        let to = self.resolve(&p.to, true).await?;
        let when = parse_optional_datetime(p.datetime.as_deref())?;
        let represents = p.datetime_represents.unwrap_or_default();
        let count = clamp(p.count, 3, 10);
        let mut q = vec![
            ("from", from.id.clone()),
            ("to", to.id.clone()),
            ("datetime", convert::request_time(&when)),
            (
                "datetime_represents",
                match represents {
                    DatetimeRepresents::Departure => "departure",
                    DatetimeRepresents::Arrival => "arrival",
                }
                .to_owned(),
            ),
            ("count", count.to_string()),
            ("data_freshness", "realtime".to_owned()),
        ];
        if let Some(max) = p.max_transfers {
            q.push(("max_nb_transfers", max.min(10).to_string()));
        }
        let res: navitia::JourneysResponse = self
            .client
            .get("journeys", &q, false)
            .await
            .or_else(empty_if_no_result)?;
        let idx = convert::index(&res.disruptions);
        Ok(PlanJourneyResult {
            from,
            to,
            datetime: time::format(&when),
            datetime_represents: represents,
            journeys: res
                .journeys
                .iter()
                .take(count as usize)
                .map(|j| convert::journey(j, &idx))
                .collect(),
        })
    }

    pub async fn departures(&self, p: BoardParams) -> Result<BoardResult, SncfError> {
        self.board(p, true).await
    }

    pub async fn arrivals(&self, p: BoardParams) -> Result<BoardResult, SncfError> {
        self.board(p, false).await
    }

    async fn board(&self, p: BoardParams, departures: bool) -> Result<BoardResult, SncfError> {
        let station = self.resolve_station(&p.station).await?;
        let since = parse_optional_datetime(p.datetime.as_deref())?;
        let count = clamp(p.count, 10, 50);
        let path = format!(
            "stop_areas/{}/{}",
            station.id,
            if departures { "departures" } else { "arrivals" }
        );
        // Note: these endpoints take `from_datetime`; `datetime` is silently ignored.
        let mut q = vec![
            ("from_datetime", convert::request_time(&since)),
            ("count", count.to_string()),
            ("data_freshness", "realtime".to_owned()),
        ];
        if let Some(minutes) = p.duration_minutes {
            q.push((
                "duration",
                (u64::from(minutes.min(24 * 60)) * 60).to_string(),
            ));
        }
        let res: navitia::BoardResponse = self
            .client
            .get(&path, &q, false)
            .await
            .or_else(empty_if_no_result)?;
        let idx = convert::index(&res.disruptions);
        let passages = if departures {
            &res.departures
        } else {
            &res.arrivals
        };
        Ok(BoardResult {
            station,
            since: time::format(&since),
            trains: passages
                .iter()
                .filter_map(|e| convert::board_entry(e, departures, &idx))
                .collect(),
        })
    }

    pub async fn train(&self, p: TrainParams) -> Result<TrainResult, SncfError> {
        let number = p.train_number.trim();
        if number.is_empty() || !number.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(SncfError::Invalid(format!(
                "invalid train number {number:?}"
            )));
        }
        let date = match p.date.as_deref() {
            Some(d) => time::parse_date(d)?,
            None => convert::today(),
        };
        let (since, until) = convert::day_window(date)
            .ok_or_else(|| SncfError::Invalid(format!("invalid date {date}")))?;
        let q = [
            ("headsign", number.to_owned()),
            ("since", convert::request_time(&since)),
            ("until", convert::request_time(&until)),
            ("count", "5".to_owned()),
            ("data_freshness", "realtime".to_owned()),
        ];
        let res: navitia::VehicleJourneysResponse = self
            .client
            .get("vehicle_journeys", &q, false)
            .await
            .or_else(empty_if_no_result)?;
        let idx = convert::index(&res.disruptions);
        Ok(TrainResult {
            date: date.to_string(),
            trains: res
                .vehicle_journeys
                .iter()
                .map(|vj| {
                    convert::train(vj, convert::service_date_of(&vj.id).unwrap_or(date), &idx)
                })
                .collect(),
        })
    }

    pub async fn disruptions(&self, p: DisruptionsParams) -> Result<DisruptionsResult, SncfError> {
        let count = clamp(p.count, 10, 25) as usize;
        let (scope, res): (String, navitia::DisruptionsResponse) = match p.station.as_deref() {
            Some(station) if !station.trim().is_empty() => {
                let station = self.resolve_station(station).await?;
                let path = format!("stop_areas/{}/traffic_reports", station.id);
                let res = self
                    .client
                    .get(&path, &[], false)
                    .await
                    .or_else(empty_if_no_result)?;
                (station.name, res)
            }
            _ => {
                let date = match p.date.as_deref() {
                    Some(d) => time::parse_date(d)?,
                    None => convert::today(),
                };
                let (start, until) = convert::day_window(date)
                    .ok_or_else(|| SncfError::Invalid(format!("invalid date {date}")))?;
                // For today, skip what is already over.
                let since = start.max(time::now());
                let q = [
                    ("since", convert::request_time(&since)),
                    ("until", convert::request_time(&until)),
                    // Fetch extra: past items are filtered out below.
                    ("count", "50".to_owned()),
                ];
                let res = self
                    .client
                    .get("disruptions", &q, false)
                    .await
                    .or_else(empty_if_no_result)?;
                ("network".to_owned(), res)
            }
        };
        Ok(DisruptionsResult {
            scope,
            disruptions: res
                .disruptions
                .iter()
                .filter(|d| d.status != "past")
                .take(count)
                .map(convert::disruption)
                .collect(),
        })
    }

    /// Accepts an id, "latitude,longitude", or free text (best match).
    async fn resolve(&self, input: &str, allow_cities: bool) -> Result<ResolvedPlace, SncfError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(SncfError::Invalid("place must not be empty".into()));
        }
        if ID_PREFIXES.iter().any(|p| input.starts_with(p)) {
            return Ok(ResolvedPlace {
                id: client::check_id(input)?.to_owned(),
                name: input.to_owned(),
            });
        }
        if let Some((lat, lon)) = parse_lat_lon(input) {
            check_coords(lat, lon)?;
            return Ok(ResolvedPlace {
                id: format!("{lon};{lat}"),
                name: input.to_owned(),
            });
        }
        let mut kinds = vec![PlaceKind::Station];
        if allow_cities {
            kinds.push(PlaceKind::City);
        }
        let found = self
            .search_places(SearchPlacesParams {
                query: input.to_owned(),
                kinds: Some(kinds),
                count: Some(10),
            })
            .await?;
        best_match(input, &found.places)
            .map(|p| ResolvedPlace {
                id: p.id.clone(),
                name: p.name.clone(),
            })
            .ok_or_else(|| SncfError::NotFound(input.to_owned()))
    }

    async fn resolve_station(&self, input: &str) -> Result<ResolvedPlace, SncfError> {
        let place = self.resolve(input, false).await?;
        if !place.id.starts_with("stop_area:") {
            return Err(SncfError::Invalid(format!(
                "{input:?} is not a station; pass a stop_area id from sncf_search_places"
            )));
        }
        Ok(place)
    }
}

/// Picks the most plausible place for free text. The API ranks fuzzily
/// ("Lyon" returns "Paris - Gare de Lyon" first), so prefer an exact name,
/// then a name starting with the query, then the API's first result.
fn best_match<'a>(query: &str, places: &'a [Place]) -> Option<&'a Place> {
    let q = normalize(query);
    places
        .iter()
        .find(|p| normalize(&p.name) == q)
        .or_else(|| places.iter().find(|p| normalize(&p.name).starts_with(&q)))
        .or_else(|| places.first())
}

/// Lowercase, strip French accents, turn punctuation into single spaces.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let mapped = match c {
            'à' | 'â' | 'ä' => "a",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'î' | 'ï' => "i",
            'ô' | 'ö' => "o",
            'ù' | 'û' | 'ü' => "u",
            'ç' => "c",
            'ÿ' => "y",
            'œ' => "oe",
            'æ' => "ae",
            c if c.is_alphanumeric() => {
                out.push(c);
                continue;
            }
            _ => " ",
        };
        out.push_str(mapped);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clamp(value: Option<u8>, default: u8, max: u8) -> u8 {
    value.unwrap_or(default).clamp(1, max)
}

fn parse_optional_datetime(
    input: Option<&str>,
) -> Result<chrono::DateTime<chrono_tz::Tz>, SncfError> {
    match input.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Ok(time::parse_datetime(s)?),
        None => Ok(time::now()),
    }
}

fn parse_lat_lon(input: &str) -> Option<(f64, f64)> {
    let (lat, lon) = input.split_once(',')?;
    Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
}

fn check_coords(lat: f64, lon: f64) -> Result<(), SncfError> {
    if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        Ok(())
    } else {
        Err(SncfError::Invalid(format!(
            "invalid coordinates {lat},{lon}"
        )))
    }
}

fn empty_if_no_result<T: Default>(e: SncfError) -> Result<T, SncfError> {
    if e.is_no_result() {
        Ok(T::default())
    } else {
        Err(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(name: &str, kind: PlaceKind) -> Place {
        Place {
            id: format!("id:{name}"),
            name: name.into(),
            kind,
            city: None,
            latitude: 0.0,
            longitude: 0.0,
            distance_m: None,
        }
    }

    #[test]
    fn best_match_prefers_exact_then_prefix() {
        let places = vec![
            place("Paris - Gare de Lyon - Hall 1 & 2", PlaceKind::Station),
            place("Lyon Part Dieu", PlaceKind::Station),
            place("Lyon", PlaceKind::City),
        ];
        assert_eq!(best_match("lyon", &places).unwrap().name, "Lyon");
        assert_eq!(
            best_match("Lyon part-dieu", &places).unwrap().name,
            "Lyon Part Dieu"
        );
        assert_eq!(best_match("Lyo", &places).unwrap().name, "Lyon Part Dieu");
        assert_eq!(best_match("xyz", &places).unwrap().name, places[0].name);
        assert!(best_match("x", &[]).is_none());
    }

    #[test]
    fn normalizes_french_names() {
        assert_eq!(
            normalize("Saint-Étienne  Châteaucreux"),
            "saint etienne chateaucreux"
        );
        assert_eq!(
            normalize("Paris - Montparnasse - Hall 1 & 2"),
            "paris montparnasse hall 1 2"
        );
    }

    #[test]
    fn lat_lon() {
        assert_eq!(parse_lat_lon("48.84, 2.32"), Some((48.84, 2.32)));
        assert_eq!(parse_lat_lon("Paris"), None);
    }

    #[test]
    fn clamps() {
        assert_eq!(clamp(None, 5, 20), 5);
        assert_eq!(clamp(Some(0), 5, 20), 1);
        assert_eq!(clamp(Some(200), 5, 20), 20);
    }
}
