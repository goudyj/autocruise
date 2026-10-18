//! The SNCF connector service: one async method per tool.

use autocruise_core::time;

use crate::client::SncfClient;
use crate::error::SncfError;
use crate::types::*;
use crate::{convert, navitia, resolve};

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
        resolve::check_coords(p.latitude, p.longitude)?;
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

    #[test]
    fn clamps() {
        assert_eq!(clamp(None, 5, 20), 5);
        assert_eq!(clamp(Some(0), 5, 20), 1);
        assert_eq!(clamp(Some(200), 5, 20), 20);
    }
}
