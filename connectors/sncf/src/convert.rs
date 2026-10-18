//! Pure conversions from Navitia responses to tool outputs.

use std::collections::HashMap;

use autocruise_core::time::{self, FRANCE_TZ};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Tz;

use crate::navitia as nv;
use crate::types::*;

/// Navitia compact local time (`20261019T080600`, Europe/Paris) -> `DateTime`.
pub(crate) fn parse_api_time(s: &str) -> Option<DateTime<Tz>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").ok()?;
    time::localize(naive).ok()
}

/// Navitia compact local time -> RFC 3339 with offset. Unknown formats pass through.
pub(crate) fn api_time(s: &str) -> String {
    parse_api_time(s).map_or_else(|| s.to_owned(), |dt| time::format(&dt))
}

/// Request parameter: always sent as UTC with a `Z` suffix, which the API
/// converts itself. Avoids any ambiguity around DST changes.
pub(crate) fn request_time(dt: &DateTime<Tz>) -> String {
    dt.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string()
}

fn minutes_between(actual: Option<&str>, base: Option<&str>) -> i64 {
    match (
        actual.and_then(parse_api_time),
        base.and_then(parse_api_time),
    ) {
        (Some(a), Some(b)) => (a - b).num_minutes(),
        _ => 0,
    }
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

fn coord(c: &nv::Coord) -> (f64, f64) {
    (c.lat.parse().unwrap_or(0.0), c.lon.parse().unwrap_or(0.0))
}

fn city_of(regions: &[nv::AdminRegion]) -> Option<String> {
    regions
        .iter()
        .find(|r| r.level == Some(8))
        .or_else(|| regions.first())
        .and_then(|r| non_empty(&r.name))
}

pub(crate) fn place(p: &nv::Place) -> Option<Place> {
    let (kind, name, c, city) = match p.embedded_type.as_str() {
        "stop_area" => {
            let sa = p.stop_area.as_ref()?;
            (
                PlaceKind::Station,
                non_empty(&sa.name).unwrap_or_else(|| p.name.clone()),
                &sa.coord,
                city_of(&sa.administrative_regions),
            )
        }
        "administrative_region" => {
            let ar = p.administrative_region.as_ref()?;
            (PlaceKind::City, p.name.clone(), &ar.coord, None)
        }
        "address" => {
            let a = p.address.as_ref()?;
            (
                PlaceKind::Address,
                p.name.clone(),
                &a.coord,
                city_of(&a.administrative_regions),
            )
        }
        "poi" => {
            let a = p.poi.as_ref()?;
            (
                PlaceKind::Poi,
                p.name.clone(),
                &a.coord,
                city_of(&a.administrative_regions),
            )
        }
        _ => return None,
    };
    let (latitude, longitude) = coord(c);
    Some(Place {
        id: p.id.clone(),
        name,
        kind,
        city,
        latitude,
        longitude,
        distance_m: p.distance.as_deref().and_then(|d| d.parse().ok()),
    })
}

pub(crate) fn navitia_type(kind: PlaceKind) -> &'static str {
    match kind {
        PlaceKind::Station => "stop_area",
        PlaceKind::City => "administrative_region",
        PlaceKind::Address => "address",
        PlaceKind::Poi => "poi",
    }
}

type DisruptionIndex<'a> = HashMap<&'a str, &'a nv::Disruption>;

pub(crate) fn index(disruptions: &[nv::Disruption]) -> DisruptionIndex<'_> {
    disruptions.iter().map(|d| (d.id.as_str(), d)).collect()
}

fn briefs<'a>(
    links: impl IntoIterator<Item = &'a nv::Link>,
    idx: &DisruptionIndex<'_>,
) -> Vec<DisruptionBrief> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for link in links {
        if link.kind != "disruption" || seen.contains(&link.id) {
            continue;
        }
        seen.push(link.id.clone());
        if let Some(d) = idx.get(link.id.as_str()) {
            out.push(brief(d));
        }
    }
    out
}

fn brief(d: &nv::Disruption) -> DisruptionBrief {
    DisruptionBrief {
        effect: d.severity.effect.clone(),
        message: d.messages.iter().find_map(|m| non_empty(&m.text)),
    }
}

fn leg(s: &nv::Section, idx: &DisruptionIndex<'_>, out: &mut Vec<DisruptionBrief>) -> Option<Leg> {
    let kind = match s.kind.as_str() {
        "public_transport" | "on_demand_transport" => LegKind::Ride,
        "street_network" => LegKind::Walk,
        "transfer" => LegKind::Transfer,
        "crow_fly" => LegKind::Access,
        "waiting" => return None,
        _ => LegKind::Other,
    };
    if kind == LegKind::Access && s.duration == 0 {
        return None;
    }
    let di = s.display_informations.as_ref();
    if let Some(di) = di {
        for b in briefs(&di.links, idx) {
            if !out.contains(&b) {
                out.push(b);
            }
        }
    }
    Some(Leg {
        kind,
        from: s.from.as_ref().and_then(|p| non_empty(&p.name)),
        to: s.to.as_ref().and_then(|p| non_empty(&p.name)),
        departure: s.departure_date_time.as_deref().map(api_time),
        arrival: s.arrival_date_time.as_deref().map(api_time),
        duration_minutes: s.duration / 60,
        mode: di
            .and_then(|d| non_empty(&d.commercial_mode))
            .or_else(|| (kind == LegKind::Walk).then(|| s.mode.clone()).flatten()),
        train_number: di
            .and_then(|d| non_empty(&d.trip_short_name).or_else(|| non_empty(&d.headsign))),
        direction: di.and_then(|d| non_empty(&d.direction)),
        delay_minutes: minutes_between(
            s.departure_date_time.as_deref(),
            s.base_departure_date_time.as_deref(),
        ),
    })
}

pub(crate) fn journey(j: &nv::Journey, idx: &DisruptionIndex<'_>) -> Journey {
    let mut disruptions = Vec::new();
    let legs = j
        .sections
        .iter()
        .filter_map(|s| leg(s, idx, &mut disruptions))
        .collect();
    Journey {
        departure: api_time(&j.departure_date_time),
        arrival: api_time(&j.arrival_date_time),
        duration_minutes: j.duration / 60,
        transfers: j.nb_transfers,
        status: non_empty(&j.status),
        co2_grams: j
            .co2_emission
            .as_ref()
            .filter(|c| c.unit == "gEC" || c.unit.starts_with('g'))
            .map(|c| (c.value * 10.0).round() / 10.0),
        legs,
        disruptions,
    }
}

pub(crate) fn board_entry(
    p: &nv::Passage,
    departures: bool,
    idx: &DisruptionIndex<'_>,
) -> Option<BoardEntry> {
    let sdt = &p.stop_date_time;
    let (actual, base) = if departures {
        (&sdt.departure_date_time, &sdt.base_departure_date_time)
    } else {
        (&sdt.arrival_date_time, &sdt.base_arrival_date_time)
    };
    let actual = actual.as_deref()?;
    let base = base.as_deref().unwrap_or(actual);
    let di = &p.display_informations;
    Some(BoardEntry {
        time: api_time(actual),
        scheduled_time: api_time(base),
        delay_minutes: minutes_between(Some(actual), Some(base)),
        realtime: sdt.data_freshness.as_deref() == Some("realtime"),
        mode: non_empty(&di.commercial_mode),
        train_number: non_empty(&di.trip_short_name).or_else(|| non_empty(&di.headsign)),
        direction: non_empty(&di.direction),
        disruptions: briefs(di.links.iter().chain(&sdt.links), idx),
    })
}

/// Builds absolute stop times from Navitia's `HHMMSS` local times: the clock
/// may wrap past midnight, in which case the following stops are on the next day.
pub(crate) fn train(
    vj: &nv::VehicleJourney,
    service_date: NaiveDate,
    idx: &DisruptionIndex<'_>,
) -> Train {
    let mut day = 0i64;
    let mut previous: Option<NaiveTime> = None;
    let mut at = |hhmmss: Option<&str>| -> Option<String> {
        let t = NaiveTime::parse_from_str(hhmmss?, "%H%M%S").ok()?;
        if previous.is_some_and(|p| t < p) {
            day += 1;
        }
        previous = Some(t);
        let naive = (service_date + Duration::days(day)).and_time(t);
        time::localize(naive).ok().map(|dt| time::format(&dt))
    };
    let stops = vj
        .stop_times
        .iter()
        .map(|st| {
            let arrival = at(st.arrival_time.as_deref());
            let departure = at(st.departure_time.as_deref());
            TrainStop {
                station: non_empty(&st.stop_point.name)
                    .unwrap_or_else(|| st.stop_point.label.clone()),
                arrival: arrival.filter(|_| st.drop_off_allowed || !st.pickup_allowed),
                departure: departure.filter(|_| st.pickup_allowed || !st.drop_off_allowed),
                skipped: st.skipped_stop,
            }
        })
        .collect();
    Train {
        train_number: non_empty(&vj.headsign).unwrap_or_else(|| vj.name.clone()),
        stops,
        disruptions: briefs(&vj.disruptions, idx),
    }
}

/// Service date encoded in vehicle journey ids (`vehicle_journey:SNCF:2026-10-19:6623:...`).
pub(crate) fn service_date_of(id: &str) -> Option<NaiveDate> {
    id.split(':')
        .find_map(|part| NaiveDate::parse_from_str(part, "%Y-%m-%d").ok())
}

pub(crate) fn disruption(d: &nv::Disruption) -> Disruption {
    Disruption {
        id: d.id.clone(),
        status: d.status.clone(),
        effect: d.severity.effect.clone(),
        message: d.messages.iter().find_map(|m| non_empty(&m.text)),
        cause: non_empty(&d.cause),
        periods: d
            .application_periods
            .iter()
            .map(|p| Period {
                begin: api_time(&p.begin),
                end: api_time(&p.end),
            })
            .collect(),
        impacted: d
            .impacted_objects
            .iter()
            .filter_map(|o| o.pt_object.as_ref())
            .map(|o| match (&o.trip, o.embedded_type.as_str()) {
                (Some(trip), _) => format!("train {}", trip.name),
                (None, "line") => format!("line {}", o.name),
                (None, "stop_area") => format!("station {}", o.name),
                _ => o.name.clone(),
            })
            .collect(),
        updated_at: non_empty(&d.updated_at).map(|u| api_time(&u)),
    }
}

/// Today's date in France.
pub(crate) fn today() -> NaiveDate {
    time::now().date_naive()
}

/// Start and end of a French calendar day, for API windows.
pub(crate) fn day_window(date: NaiveDate) -> Option<(DateTime<Tz>, DateTime<Tz>)> {
    let start = time::localize(date.and_time(NaiveTime::MIN)).ok()?;
    let end = time::localize(date.and_hms_opt(23, 59, 59)?).ok()?;
    Some((start.with_timezone(&FRANCE_TZ), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture<T: serde::de::DeserializeOwned>(json: &str) -> T {
        serde_json::from_str(json).expect("fixture parses")
    }

    #[test]
    fn api_times_are_paris_local() {
        assert_eq!(api_time("20261019T080600"), "2026-10-19T08:06:00+02:00");
        assert_eq!(api_time("20261201T080600"), "2026-12-01T08:06:00+01:00");
        // Autumn DST overlap resolves to the earliest instant.
        assert_eq!(api_time("20261025T023000"), "2026-10-25T02:30:00+02:00");
        assert_eq!(api_time("garbage"), "garbage");
    }

    #[test]
    fn requests_are_sent_in_utc() {
        let dt = time::parse_datetime("2026-10-19T08:00").unwrap();
        assert_eq!(request_time(&dt), "20261019T060000Z");
        let winter = time::parse_datetime("2026-12-01T08:00").unwrap();
        assert_eq!(request_time(&winter), "20261201T070000Z");
    }

    #[test]
    fn places_fixture() {
        let res: nv::PlacesResponse = fixture(include_str!("../tests/fixtures/places.json"));
        let places: Vec<_> = res.places.iter().filter_map(place).collect();
        assert_eq!(places[0].id, "stop_area:SNCF:87723197");
        assert_eq!(places[0].name, "Lyon Part Dieu");
        assert_eq!(places[0].kind, PlaceKind::Station);
        assert_eq!(places[0].city.as_deref(), Some("Lyon"));
        assert!((places[0].latitude - 45.76).abs() < 0.01);
    }

    #[test]
    fn nearby_fixture() {
        let res: nv::PlacesResponse = fixture(include_str!("../tests/fixtures/places_nearby.json"));
        let first = place(&res.places_nearby[0]).unwrap();
        assert_eq!(first.id, "stop_area:SNCF:87391003");
        assert_eq!(first.distance_m, Some(50));
    }

    #[test]
    fn journeys_fixture() {
        let res: nv::JourneysResponse = fixture(include_str!("../tests/fixtures/journeys.json"));
        let idx = index(&res.disruptions);
        let j = journey(&res.journeys[0], &idx);
        assert_eq!(j.departure, "2026-10-19T08:06:00+02:00");
        assert_eq!(j.arrival, "2026-10-19T10:29:00+02:00");
        assert_eq!(j.transfers, 1);
        // Zero-length access legs and waiting sections are dropped.
        let kinds: Vec<_> = j.legs.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [LegKind::Ride, LegKind::Transfer, LegKind::Ride]);
        let tgv = &j.legs[2];
        assert_eq!(tgv.mode.as_deref(), Some("TGV INOUI"));
        assert_eq!(tgv.train_number.as_deref(), Some("12510"));
        assert_eq!(tgv.to.as_deref(), Some("Lyon Part Dieu (Lyon)"));
        assert_eq!(tgv.duration_minutes, 104);
    }

    #[test]
    fn departures_fixture() {
        let res: nv::BoardResponse = fixture(include_str!("../tests/fixtures/departures.json"));
        let idx = index(&res.disruptions);
        let board: Vec<_> = res
            .departures
            .iter()
            .filter_map(|p| board_entry(p, true, &idx))
            .collect();
        assert_eq!(board.len(), 4);
        assert_eq!(board[0].time, "2026-10-19T08:01:00+02:00");
        assert_eq!(board[0].train_number.as_deref(), Some("8423"));
        // Prefer the train number over the Transilien mission code.
        assert_eq!(board[2].train_number.as_deref(), Some("164565"));
        assert!(board.iter().all(|b| b.time.ends_with("+02:00")));
    }

    #[test]
    fn arrivals_fixture() {
        let res: nv::BoardResponse = fixture(include_str!("../tests/fixtures/arrivals.json"));
        let idx = index(&res.disruptions);
        let first = board_entry(&res.arrivals[0], false, &idx).unwrap();
        assert_eq!(first.time, "2026-10-19T08:04:00+02:00");
    }

    #[test]
    fn delays_and_disruption_links() {
        let mut p: nv::Passage = Default::default();
        p.stop_date_time.departure_date_time = Some("20261019T081200".into());
        p.stop_date_time.base_departure_date_time = Some("20261019T080100".into());
        p.stop_date_time.data_freshness = Some("realtime".into());
        p.display_informations.links = vec![nv::Link {
            kind: "disruption".into(),
            id: "d1".into(),
        }];
        let d = nv::Disruption {
            id: "d1".into(),
            severity: nv::Severity {
                name: "trip delayed".into(),
                effect: "SIGNIFICANT_DELAYS".into(),
            },
            messages: vec![nv::Message {
                text: "Panne de signalisation".into(),
            }],
            ..Default::default()
        };
        let all = [d];
        let e = board_entry(&p, true, &index(&all)).unwrap();
        assert_eq!(e.delay_minutes, 11);
        assert!(e.realtime);
        assert_eq!(e.scheduled_time, "2026-10-19T08:01:00+02:00");
        assert_eq!(e.disruptions[0].effect, "SIGNIFICANT_DELAYS");
        assert_eq!(
            e.disruptions[0].message.as_deref(),
            Some("Panne de signalisation")
        );
    }

    #[test]
    fn train_fixture() {
        let res: nv::VehicleJourneysResponse =
            fixture(include_str!("../tests/fixtures/vehicle_journeys.json"));
        let vj = &res.vehicle_journeys[0];
        let date = service_date_of(&vj.id).unwrap();
        assert_eq!(date.to_string(), "2026-10-19");
        let t = train(vj, date, &index(&res.disruptions));
        assert_eq!(t.train_number, "6623");
        assert_eq!(t.stops.len(), 3);
        assert_eq!(t.stops[0].arrival, None);
        assert_eq!(
            t.stops[0].departure.as_deref(),
            Some("2026-10-19T17:00:00+02:00")
        );
        assert_eq!(
            t.stops[1].arrival.as_deref(),
            Some("2026-10-19T18:56:00+02:00")
        );
        assert_eq!(t.stops[2].departure, None);
    }

    #[test]
    fn train_crossing_midnight() {
        let stop = |arr: &str, dep: &str| nv::StopTime {
            arrival_time: Some(arr.into()),
            departure_time: Some(dep.into()),
            pickup_allowed: true,
            drop_off_allowed: true,
            ..Default::default()
        };
        let vj = nv::VehicleJourney {
            headsign: "3751".into(),
            stop_times: vec![stop("232000", "233000"), stop("001500", "002000")],
            ..Default::default()
        };
        let date = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let t = train(&vj, date, &index(&[]));
        assert_eq!(
            t.stops[0].departure.as_deref(),
            Some("2026-10-19T23:30:00+02:00")
        );
        assert_eq!(
            t.stops[1].arrival.as_deref(),
            Some("2026-10-20T00:15:00+02:00")
        );
    }

    #[test]
    fn disruptions_fixture() {
        let res: nv::DisruptionsResponse =
            fixture(include_str!("../tests/fixtures/disruptions.json"));
        let d = disruption(&res.disruptions[0]);
        assert_eq!(d.effect, "NO_SERVICE");
        assert_eq!(d.impacted, ["train 858385"]);
        assert_eq!(d.periods[0].begin, "2026-10-17T18:35:00+02:00");
        assert_eq!(
            d.message.as_deref(),
            Some("Occupation des voies par des manifestants")
        );
        assert_eq!(d.cause, None);
    }

    #[test]
    fn traffic_reports_fixture() {
        let res: nv::DisruptionsResponse =
            fixture(include_str!("../tests/fixtures/traffic_reports.json"));
        let impacted: Vec<_> = res
            .disruptions
            .iter()
            .map(disruption)
            .flat_map(|d| d.impacted)
            .collect();
        assert_eq!(impacted, ["train 8301", "train 8317", "train 8314"]);
    }
}
