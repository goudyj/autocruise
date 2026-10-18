//! Request-construction tests: the connector is driven through a mock
//! transport, so we can assert exactly what would be sent to the SNCF API.

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::error::{Classify, ErrorKind};
use autocruise_core::http::mock::MockTransport;
use autocruise_sncf::types::*;
use autocruise_sncf::{Sncf, SncfClient};

const BASE: &str = "http://mock.test/v1/coverage/sncf";
const PLACES: &str = include_str!("fixtures/places.json");
const DEPARTURES: &str = include_str!("fixtures/departures.json");
const ARRIVALS: &str = include_str!("fixtures/arrivals.json");
const JOURNEYS: &str = include_str!("fixtures/journeys.json");
const VEHICLE_JOURNEYS: &str = include_str!("fixtures/vehicle_journeys.json");
const TRAFFIC_REPORTS: &str = include_str!("fixtures/traffic_reports.json");
const MONTPARNASSE: &str = "stop_area:SNCF:87391003";
const PART_DIEU: &str = "stop_area:SNCF:87723197";

fn sncf_with(mock: &Arc<MockTransport>, token: Option<&str>) -> Sncf {
    Sncf::new(
        SncfClient::new(mock.clone(), token.map(String::from), BASE)
            .with_min_interval(Duration::ZERO),
    )
}

fn sncf(mock: &Arc<MockTransport>) -> Sncf {
    sncf_with(mock, Some("test-token"))
}

fn board(station: &str, datetime: Option<&str>) -> BoardParams {
    BoardParams {
        station: station.into(),
        datetime: datetime.map(String::from),
        count: None,
        duration_minutes: None,
    }
}

#[tokio::test]
async fn departures_send_from_datetime_in_utc() {
    let path = format!("/stop_areas/{MONTPARNASSE}/departures");
    let mock = Arc::new(MockTransport::new().route(&path, &[], 200, DEPARTURES));
    let params = BoardParams {
        count: Some(4),
        duration_minutes: Some(30),
        ..board(MONTPARNASSE, Some("2026-10-19T08:00"))
    };
    let res = sncf(&mock).departures(params).await.unwrap();

    assert_eq!(res.since, "2026-10-19T08:00:00+02:00");
    assert_eq!(res.trains.len(), 4);
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!(r.header_value("authorization"), Some("test-token"));
    // The boards take from_datetime; datetime would be silently ignored by the API.
    assert_eq!(
        r.query("from_datetime").as_deref(),
        Some("20261019T060000Z")
    );
    assert_eq!(r.query("datetime"), None);
    assert_eq!(r.query("duration").as_deref(), Some("1800"));
    assert_eq!(r.query("count").as_deref(), Some("4"));
    assert_eq!(r.query("data_freshness").as_deref(), Some("realtime"));
}

#[tokio::test]
async fn arrivals_resolve_a_station_name_first() {
    let mock = Arc::new(
        MockTransport::new()
            .route("/places", &[], 200, PLACES)
            .route(
                &format!("/stop_areas/{PART_DIEU}/arrivals"),
                &[],
                200,
                ARRIVALS,
            ),
    );
    let res = sncf(&mock)
        .arrivals(board("Lyon Part-Dieu", Some("2026-10-19T06:00:00Z")))
        .await
        .unwrap();

    assert_eq!(res.station.id, PART_DIEU);
    assert_eq!(res.station.name, "Lyon Part Dieu");
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].query("q").as_deref(), Some("Lyon Part-Dieu"));
    // Boards only accept stations: cities are not searched.
    assert_eq!(reqs[0].query_all("type[]"), ["stop_area"]);
    assert_eq!(
        reqs[1].query("from_datetime").as_deref(),
        Some("20261019T060000Z")
    );
}

#[tokio::test]
async fn plan_journey_builds_the_request() {
    let mock = Arc::new(MockTransport::new().route("/journeys", &[], 200, JOURNEYS));
    let res = sncf(&mock)
        .plan_journey(PlanJourneyParams {
            from: "admin:fr:75056".into(),
            to: PART_DIEU.into(),
            datetime: Some("2026-12-01T11:00".into()),
            datetime_represents: Some(DatetimeRepresents::Arrival),
            count: Some(1),
            max_transfers: Some(1),
        })
        .await
        .unwrap();

    // Winter time: +01:00.
    assert_eq!(res.datetime, "2026-12-01T11:00:00+01:00");
    assert_eq!(res.journeys.len(), 1);
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 1, "ids are not searched");
    let r = &reqs[0];
    assert_eq!(r.query("from").as_deref(), Some("admin:fr:75056"));
    assert_eq!(r.query("to").as_deref(), Some(PART_DIEU));
    assert_eq!(r.query("datetime").as_deref(), Some("20261201T100000Z"));
    assert_eq!(r.query("datetime_represents").as_deref(), Some("arrival"));
    assert_eq!(r.query("max_nb_transfers").as_deref(), Some("1"));
}

#[tokio::test]
async fn coordinates_become_lon_lat() {
    let mock = Arc::new(MockTransport::new().route("/journeys", &[], 200, JOURNEYS));
    sncf(&mock)
        .plan_journey(PlanJourneyParams {
            from: "48.8412, 2.3212".into(),
            to: PART_DIEU.into(),
            datetime: Some("2026-10-19T08:00".into()),
            datetime_represents: None,
            count: None,
            max_transfers: None,
        })
        .await
        .unwrap();
    let r = &mock.requests()[0];
    assert_eq!(r.query("from").as_deref(), Some("2.3212;48.8412"));
    assert_eq!(r.query("datetime_represents").as_deref(), Some("departure"));
}

#[tokio::test]
async fn train_queries_the_whole_french_day() {
    let mock = Arc::new(MockTransport::new().route(
        "/vehicle_journeys",
        &[("headsign", "6623")],
        200,
        VEHICLE_JOURNEYS,
    ));
    let res = sncf(&mock)
        .train(TrainParams {
            train_number: "6623".into(),
            date: Some("2026-10-19".into()),
        })
        .await
        .unwrap();

    assert_eq!(res.trains[0].stops.len(), 3);
    let r = &mock.requests()[0];
    assert_eq!(r.query("since").as_deref(), Some("20261018T220000Z"));
    assert_eq!(r.query("until").as_deref(), Some("20261019T215959Z"));
}

#[tokio::test]
async fn station_disruptions_use_traffic_reports() {
    let mock = Arc::new(
        MockTransport::new()
            .route("/places", &[], 200, PLACES)
            .route(
                &format!("/stop_areas/{MONTPARNASSE}/traffic_reports"),
                &[],
                200,
                TRAFFIC_REPORTS,
            ),
    );
    let res = sncf(&mock)
        .disruptions(DisruptionsParams {
            station: Some(MONTPARNASSE.into()),
            date: None,
            count: Some(2),
        })
        .await
        .unwrap();
    assert_eq!(res.disruptions.len(), 2);
    assert!(res.disruptions.iter().all(|d| d.status == "future"));
    assert_eq!(mock.requests().len(), 1);
}

#[tokio::test]
async fn place_searches_are_cached() {
    let mock = Arc::new(MockTransport::new().route("/places", &[], 200, PLACES));
    let s = sncf(&mock);
    let params = || SearchPlacesParams {
        query: "lyon part dieu".into(),
        kinds: None,
        count: None,
    };
    s.search_places(params()).await.unwrap();
    s.search_places(params()).await.unwrap();
    assert_eq!(mock.requests().len(), 1);
}

#[tokio::test]
async fn missing_token_fails_before_any_request() {
    let mock = Arc::new(MockTransport::new());
    let err = sncf_with(&mock, None)
        .departures(board(MONTPARNASSE, None))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingCredentials);
    assert!(mock.requests().is_empty());
}

#[tokio::test]
async fn invalid_inputs_fail_before_any_request() {
    let mock = Arc::new(MockTransport::new());
    let s = sncf(&mock);
    let err = s
        .departures(board("stop_area:../x", None))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let err = s
        .departures(board(MONTPARNASSE, Some("2026-03-29T02:30")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(mock.requests().is_empty());
}

#[tokio::test]
async fn http_errors_are_classified() {
    let unauthorized =
        Arc::new(MockTransport::new().route("/journeys", &[], 401, r#"{"message":"no token"}"#));
    let bad_request = Arc::new(MockTransport::new().route(
        "/journeys",
        &[],
        400,
        r#"{"message":"your origin and destination points are the same"}"#,
    ));
    let params = || PlanJourneyParams {
        from: PART_DIEU.into(),
        to: PART_DIEU.into(),
        datetime: None,
        datetime_represents: None,
        count: None,
        max_transfers: None,
    };
    let err = sncf(&unauthorized)
        .plan_journey(params())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unauthorized);
    let err = sncf(&bad_request).plan_journey(params()).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.to_string().contains("origin and destination"));
}

#[tokio::test]
async fn no_solution_is_an_empty_result() {
    let mock = Arc::new(MockTransport::new().route(
        "/journeys",
        &[],
        404,
        r#"{"error":{"id":"no_solution","message":"no solution found for this journey"}}"#,
    ));
    let res = sncf(&mock)
        .plan_journey(PlanJourneyParams {
            from: "admin:fr:75056".into(),
            to: PART_DIEU.into(),
            datetime: None,
            datetime_represents: None,
            count: None,
            max_transfers: None,
        })
        .await
        .unwrap();
    assert!(res.journeys.is_empty());
}

#[tokio::test]
async fn unknown_station_name_is_not_found() {
    let mock = Arc::new(MockTransport::new().route("/places", &[], 200, r#"{"places":[]}"#));
    let err = sncf(&mock)
        .departures(board("Nowhere", None))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
}
