//! Turning user input into API place ids: ids pass through, `latitude,longitude`
//! becomes a Navitia coordinate, free text goes through a place search.

use crate::client::check_id;
use crate::error::SncfError;
use crate::service::Sncf;
use crate::types::{Place, PlaceKind, ResolvedPlace, SearchPlacesParams};

const ID_PREFIXES: &[&str] = &["stop_area:", "stop_point:", "admin:", "address:", "poi:"];

impl Sncf {
    /// Accepts an id, "latitude,longitude", or free text (best match).
    pub(crate) async fn resolve(
        &self,
        input: &str,
        allow_cities: bool,
    ) -> Result<ResolvedPlace, SncfError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(SncfError::Invalid("place must not be empty".into()));
        }
        if ID_PREFIXES.iter().any(|p| input.starts_with(p)) {
            return Ok(ResolvedPlace {
                id: check_id(input)?.to_owned(),
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

    pub(crate) async fn resolve_station(&self, input: &str) -> Result<ResolvedPlace, SncfError> {
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
pub(crate) fn best_match<'a>(query: &str, places: &'a [Place]) -> Option<&'a Place> {
    let q = normalize(query);
    places
        .iter()
        .find(|p| normalize(&p.name) == q)
        .or_else(|| places.iter().find(|p| normalize(&p.name).starts_with(&q)))
        .or_else(|| places.first())
}

/// Lowercase, strip French accents, turn punctuation into single spaces.
pub(crate) fn normalize(s: &str) -> String {
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

pub(crate) fn parse_lat_lon(input: &str) -> Option<(f64, f64)> {
    let (lat, lon) = input.split_once(',')?;
    Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
}

pub(crate) fn check_coords(lat: f64, lon: f64) -> Result<(), SncfError> {
    if (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon) {
        Ok(())
    } else {
        Err(SncfError::Invalid(format!(
            "invalid coordinates {lat},{lon}"
        )))
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
        assert!(check_coords(48.8, 2.3).is_ok());
        assert!(check_coords(91.0, 2.3).is_err());
    }
}
