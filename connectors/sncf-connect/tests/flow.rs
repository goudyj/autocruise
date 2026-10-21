//! End-to-end flows against a mock desktop: recorded (sanitized)
//! accessibility trees of sncf-connect.com, with transitions on inputs.
//! Nothing here touches a display or the network.

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::error::{Classify, ErrorKind};
use autocruise_desktop::mock::{MockDesktop, Trigger};
use autocruise_desktop::{Input, Node, Rect};
use autocruise_sncf_connect::types::{
    HoldFareParams, PricesParams, PricesResult, TrainType, TravelClass,
};
use autocruise_sncf_connect::{SncfConnect, SncfConnectError, Timing};
use chrono::NaiveDate;

const FORM: &str = include_str!("fixtures/form.json");
const DEPARTURE_SUGGESTIONS: &str = include_str!("fixtures/departure_suggestions.json");
const DEPARTURE_SET: &str = include_str!("fixtures/departure_set.json");
const ARRIVAL_SUGGESTIONS: &str = include_str!("fixtures/arrival_suggestions.json");
const ARRIVAL_SET: &str = include_str!("fixtures/arrival_set.json");
const DATE_DIALOG: &str = include_str!("fixtures/date_dialog.json");
const TIME_SLOTS: &str = include_str!("fixtures/time_slots.json");
const TIME_CHOSEN: &str = include_str!("fixtures/time_chosen.json");
const DATE_SET: &str = include_str!("fixtures/date_set.json");
const RESULTS: &str = include_str!("fixtures/results.json");
// Paris -> Lyon on Monday 26 October 2026 from 2 PM: first page, after
// "Display the next journeys", after "Display previous journeys" too.
const RESULTS_2PM: &str = include_str!("fixtures/results_2pm.json");
const RESULTS_2PM_NEXT: &str = include_str!("fixtures/results_2pm_next.json");
const RESULTS_2PM_PREVIOUS: &str = include_str!("fixtures/results_2pm_previous.json");
/// "This journey in detail" of the 17:00 TGV INOUI 6623.
const JOURNEY_DETAIL: &str = include_str!("fixtures/journey_detail.json");

const SEARCH_URL: &str = "https://www.sncf-connect.com/en-en/home/search/od";
const TEN_AM: &str = "Select your fare : Departure from Paris - Gare De Lyon - Hall 1 & 2 10:00";

fn parse(json: &str) -> Node {
    serde_json::from_str(json).expect("fixture")
}

/// Recorded screens of a Paris -> Lyon search on 2026-10-21 from 8 AM.
fn search_screens() -> Vec<Node> {
    [
        FORM,
        DEPARTURE_SUGGESTIONS,
        DEPARTURE_SET,
        ARRIVAL_SUGGESTIONS,
        ARRIVAL_SET,
        DATE_DIALOG,
        TIME_SLOTS,
        TIME_CHOSEN,
        DATE_SET,
        RESULTS,
    ]
    .iter()
    .map(|s| parse(s))
    .collect()
}

fn search_transitions(mock: MockDesktop) -> MockDesktop {
    let click = |s: &str| Trigger::Click(s.to_owned());
    let typed = |s: &str| Trigger::Type(s.to_owned());
    mock.transition(Some(0), typed("Paris"), 1)
        .transition(Some(1), click("Paris : City"), 2)
        .transition(Some(2), typed("Lyon"), 3)
        .transition(Some(3), click("Lyon : City"), 4)
        .transition(Some(4), click("Outbound:"), 5)
        .transition(Some(5), click("Outbound journey time"), 6)
        .transition(Some(6), click("8 AM"), 7)
        .transition(Some(7), click("Confirm"), 8)
        .transition(Some(8), click("See prices"), 9)
}

fn timing() -> Timing {
    Timing {
        input_interval: Duration::ZERO,
        search_interval: Duration::ZERO,
        poll: Duration::from_millis(50),
        page_timeout: Duration::from_secs(10),
        cache_ttl: Duration::from_secs(300),
    }
}

fn service(mock: &Arc<MockDesktop>) -> SncfConnect {
    SncfConnect::with_timing(mock.clone(), timing())
        .with_today(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap())
}

fn paris_lyon() -> PricesParams {
    PricesParams {
        from: "Paris".into(),
        to: "Lyon".into(),
        date: "2026-10-21".into(),
        time_after: Some("08:00".into()),
        time_before: None,
        passengers: None,
        direct_only: None,
        train_types: None,
    }
}

fn typed(mock: &MockDesktop) -> Vec<String> {
    mock.inputs()
        .into_iter()
        .filter_map(|i| match i {
            Input::Type(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn prices_drive_the_form_and_read_the_results() {
    let mock = Arc::new(search_transitions(MockDesktop::new(search_screens())));
    let sncf = service(&mock);
    let res = sncf.prices(paris_lyon()).await.unwrap();

    assert_eq!(typed(&mock), [SEARCH_URL, "Paris", "Lyon", "21/10/2026"]);
    assert_eq!(mock.current(), 9);
    assert_eq!(
        (res.origin.as_str(), res.destination.as_str()),
        ("Paris", "Lyon")
    );
    assert_eq!(res.searched_from, "2026-10-21T08:00:00+02:00");
    assert_eq!(res.currency, "EUR");
    assert_eq!(res.journeys.len(), 7);
    assert!(res.more_later);

    let first = &res.journeys[0];
    assert_eq!(first.departure, "2026-10-21T09:42:00+02:00");
    assert!(first.fully_booked);
    assert_eq!(first.price_second_class, None);

    let inoui = &res.journeys[1];
    assert_eq!(inoui.departure, "2026-10-21T10:00:00+02:00");
    assert_eq!(inoui.arrival, "2026-10-21T11:56:00+02:00");
    assert_eq!(inoui.duration_minutes, Some(116));
    assert_eq!(inoui.train_type.as_deref(), Some("TGV INOUI"));
    assert_eq!(inoui.origin_station, "Paris - Gare De Lyon - Hall 1 & 2");
    assert_eq!(inoui.destination_station, "Lyon Part Dieu");
    assert!(inoui.direct);
    assert_eq!(inoui.price_second_class, Some(115.0));
    assert_eq!(inoui.price_first_class, Some(158.0));
    assert_eq!(inoui.seats_left_second_class, Some(4));
    assert_eq!(inoui.seats_left_first_class, Some(2));

    let ouigo = &res.journeys[3];
    assert_eq!(ouigo.price_second_class, Some(59.0));
    assert_eq!(
        ouigo.price_first_class, None,
        "OUIGO shows '-' in first class"
    );

    assert_eq!(res.cheapest_per_day.len(), 7);
    assert_eq!(res.cheapest_per_day[1].date, "2026-10-21");
    assert_eq!(res.cheapest_per_day[1].cheapest_price, Some(25.0));
    assert_eq!(res.cheapest_per_day[0].cheapest_price, None);
}

#[tokio::test(start_paused = true)]
async fn results_are_cached() {
    let mock = Arc::new(search_transitions(MockDesktop::new(search_screens())));
    let sncf = service(&mock);
    let first = sncf.prices(paris_lyon()).await.unwrap();
    let (inputs, snapshots) = (mock.inputs().len(), mock.snapshots());
    let again = sncf.prices(paris_lyon()).await.unwrap();
    assert_eq!(first, again);
    assert_eq!(mock.inputs().len(), inputs);
    assert_eq!(mock.snapshots(), snapshots);
}

fn blocked_screen() -> Node {
    Node::new("application", "Google Chrome").with_children(vec![
        Node::new("entry", "Address and search bar").with_bounds(Rect::new(158, 51, 1578, 24)),
        Node::new("document web", "sncf-connect.com")
            .with_bounds(Rect::new(0, 87, 1919, 992))
            .with_children(vec![
                Node::new("heading", "Access is temporarily restricted"),
                Node::new(
                    "static",
                    "We detected unusual activity from your device or network.",
                ),
            ]),
    ])
}

#[tokio::test(start_paused = true)]
async fn a_block_page_stops_everything_before_any_input() {
    let mock = Arc::new(MockDesktop::new(vec![blocked_screen()]));
    let err = service(&mock).prices(paris_lyon()).await.unwrap_err();
    assert!(matches!(err, SncfConnectError::Challenge(_)), "{err}");
    assert_eq!(err.kind(), ErrorKind::UserActionRequired);
    assert!(err.to_string().contains("complete or wait it out yourself"));
    assert!(mock.inputs().is_empty(), "never touches a challenge");
}

#[tokio::test(start_paused = true)]
async fn a_challenge_after_submitting_stops_without_retrying() {
    let mut screens = search_screens();
    screens.push(blocked_screen());
    screens[10].children[1].children[0].name = "Verification Required".into();
    // Registered first, so it wins over the normal See prices transition.
    let mock = Arc::new(search_transitions(MockDesktop::new(screens).transition(
        Some(8),
        Trigger::Click("See prices".into()),
        10,
    )));
    let err = service(&mock).prices(paris_lyon()).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UserActionRequired);
    let last = mock.inputs().last().cloned().unwrap();
    assert!(
        matches!(last, Input::Click { .. }),
        "stopped right after See prices"
    );
    assert_eq!(typed(&mock).len(), 4, "no new search");
}

#[tokio::test(start_paused = true)]
async fn logged_out_is_reported() {
    let form = FORM.replace(
        r#""role":"link","name":"My Account""#,
        r#""role":"button","name":"Sign in""#,
    );
    let mock = Arc::new(MockDesktop::new(vec![parse(&form)]));
    let err = service(&mock).prices(paris_lyon()).await.unwrap_err();
    assert!(matches!(err, SncfConnectError::LoggedOut), "{err}");
    assert_eq!(err.kind(), ErrorKind::UserActionRequired);
    assert_eq!(
        typed(&mock),
        [SEARCH_URL],
        "stopped before filling the form"
    );
}

#[tokio::test(start_paused = true)]
async fn unknown_places_are_not_found() {
    let mock = Arc::new(MockDesktop::new(vec![parse(FORM)]));
    let err = service(&mock)
        .prices(PricesParams {
            from: "Zzzzville".into(),
            ..paris_lyon()
        })
        .await
        .unwrap_err();
    assert!(matches!(err, SncfConnectError::UnknownPlace(_)), "{err}");
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[tokio::test(start_paused = true)]
async fn invalid_input_never_touches_the_browser() {
    let mock = Arc::new(MockDesktop::new(vec![parse(FORM)]));
    let err = service(&mock)
        .prices(PricesParams {
            date: "2026-10-19".into(),
            ..paris_lyon()
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(mock.snapshots(), 0);
}

// ---- Fare hold (free option). Basket pages are synthetic: the live check
// only went as far as reading the basket, never placing an option.

fn page(title: &str, children: Vec<Node>) -> Node {
    Node::new("application", "Google Chrome").with_children(vec![
        Node::new("entry", "Address and search bar").with_bounds(Rect::new(158, 51, 1578, 24)),
        Node::new("document web", title)
            .with_bounds(Rect::new(0, 87, 1919, 992))
            .with_children(children),
    ])
}

fn button(name: &str, y: i32) -> Node {
    Node::new("button", name).with_bounds(Rect::new(400, y, 300, 48))
}

/// Results with the 10:00 fares expanded.
fn expanded_results() -> Node {
    let mut tree = parse(RESULTS);
    fn expand(n: &mut Node) -> bool {
        let is_ten = n.role == "list item"
            && n.iter()
                .any(|c| c.role == "heading" && c.name.contains("10:00"));
        if is_ten {
            n.children.push(
                Node::new(
                    "heading",
                    "Second Class : €115, Modifiable subject to conditions.",
                )
                .with_bounds(Rect::new(343, 846, 1, 1)),
            );
            n.children.push(button("Choose this fare", 900));
            n.children.push(
                Node::new(
                    "heading",
                    "1st Class : €158, Modifiable subject to conditions.",
                )
                .with_bounds(Rect::new(659, 846, 1, 1)),
            );
            n.children.push(
                Node::new("button", "Choose this fare").with_bounds(Rect::new(760, 900, 268, 48)),
            );
            return true;
        }
        n.children.iter_mut().any(expand)
    }
    assert!(expand(&mut tree));
    tree
}

fn basket(option: Vec<Node>) -> Node {
    let mut children = vec![
        Node::new("heading", "Basket"),
        Node::new("radio button", "Allianz Travel Yes, insure this trip €4.20")
            .with_bounds(Rect::new(332, 300, 48, 49)),
        Node::new("radio button", "No, do not insure this trip")
            .with_bounds(Rect::new(757, 300, 49, 48)),
        button(
            "Remove from cart From Paris Gare De Lyon to Lyon Part Dieu",
            500,
        ),
        button("Confirm and pay", 600),
    ];
    children.extend(option);
    page("SNCF Connect - Basket", children)
}

fn hold_screens(offered: bool) -> Vec<Node> {
    let mut screens = search_screens();
    screens.push(expanded_results()); // 10
    screens.push(page(
        "SNCF Connect - Services",
        vec![
            Node::new("heading", "Travel personalization"),
            button("Skip step", 448),
        ],
    )); // 11
    let option = if offered {
        vec![button("Place an option for free", 700)]
    } else {
        vec![
            Node::new("static", "Place an option for free"),
            Node::new("static", "Option not applicable on this order"),
        ]
    };
    screens.push(basket(option)); // 12
    let mut confirm_removal = basket(Vec::new());
    confirm_removal.children[1].children.push(
        Node::new(
            "dialog",
            "Are you sure that you want to remove this journey from the cart?",
        )
        .with_children(vec![button("Yes, remove it", 800)]),
    );
    screens.push(confirm_removal); // 13
    screens.push(page(
        "SNCF Connect - Basket",
        vec![Node::new("heading", "Your basket is currently empty.")],
    )); // 14
    screens.push(page(
        "SNCF Connect - Option",
        vec![button("Confirm your option", 700)],
    )); // 15
    screens.push(page(
        "SNCF Connect - Confirmation",
        vec![Node::new(
            "static",
            "Your option is registered: pay before 23 October 2026 at 11:59 PM",
        )],
    )); // 16
    // The hold searches from the train's time: the 10 AM slot.
    let ten = TIME_CHOSEN
        .replace(
            "Outbound journey time, 8 AM",
            "Outbound journey time, 10 AM",
        )
        .replace(r#""value":"8 AM""#, r#""value":"10 AM""#);
    screens.push(parse(&ten)); // 17
    screens
}

fn hold_mock(offered: bool) -> (Arc<MockDesktop>, Vec<Node>) {
    let screens = hold_screens(offered);
    let click = |s: &str| Trigger::Click(s.to_owned());
    let mock = search_transitions(MockDesktop::new(screens.clone()))
        .transition(Some(6), click("10 AM"), 17)
        .transition(Some(17), click("Confirm"), 8)
        .transition(Some(9), click(TEN_AM), 10)
        .transition(Some(10), click("Choose this fare"), 11)
        .transition(Some(11), click("Skip step"), 12)
        .transition(Some(12), click("Remove from cart"), 13)
        .transition(Some(13), click("Yes, remove it"), 14)
        .transition(Some(12), click("Place an option for free"), 15)
        .transition(Some(15), click("Confirm your option"), 16);
    (Arc::new(mock), screens)
}

fn hold(confirm: Option<bool>) -> HoldFareParams {
    HoldFareParams {
        from: "Paris".into(),
        to: "Lyon".into(),
        date: "2026-10-21".into(),
        departure_time: "10:00".into(),
        travel_class: None,
        train_number: None,
        passengers: None,
        confirm,
    }
}

#[tokio::test(start_paused = true)]
async fn hold_without_confirm_is_a_preview_that_changes_nothing() {
    let (mock, _) = hold_mock(true);
    let res = service(&mock).hold_fare(hold(None)).await.unwrap();
    assert_eq!(res.status, "preview");
    assert_eq!(res.price, Some(115.0));
    assert_eq!(res.travel_class, TravelClass::Second);
    assert_eq!(res.journey.departure, "2026-10-21T10:00:00+02:00");
    assert_eq!(mock.current(), 9, "stayed on the results page");
    let clicked = mock.clicked();
    assert!(
        !clicked
            .iter()
            .any(|n| n.starts_with("Select your fare") || n == "Choose this fare")
    );
}

#[tokio::test(start_paused = true)]
async fn hold_places_the_free_option_and_never_pays() {
    let (mock, _) = hold_mock(true);
    let res = service(&mock).hold_fare(hold(Some(true))).await.unwrap();
    assert_eq!(res.status, "held");
    assert_eq!(res.price, Some(115.0));
    assert_eq!(
        res.confirmation,
        ["Your option is registered: pay before 23 October 2026 at 11:59 PM"]
    );
    let clicked = mock.clicked();
    for expected in [
        "Skip step",
        "No, do not insure this trip",
        "Place an option for free",
        "Confirm your option",
    ] {
        assert!(
            clicked.iter().any(|n| n == expected),
            "{expected} in {clicked:?}"
        );
    }
    assert!(
        !clicked
            .iter()
            .any(|n| n.contains("pay") && n != "Confirm your option")
    );
    assert!(!clicked.iter().any(|n| n.contains("insure this trip €")));
}

#[tokio::test(start_paused = true)]
async fn hold_refused_by_sncf_connect_empties_the_basket_again() {
    let (mock, _) = hold_mock(false);
    let err = service(&mock)
        .hold_fare(hold(Some(true)))
        .await
        .unwrap_err();
    assert!(matches!(err, SncfConnectError::HoldNotOffered(_)), "{err}");
    assert!(
        err.to_string()
            .contains("Option not applicable on this order")
    );
    assert_eq!(mock.current(), 14, "trip removed from the basket");
    let clicked = mock.clicked();
    assert!(!clicked.iter().any(|n| n == "Confirm and pay"));
}

#[tokio::test(start_paused = true)]
async fn hold_of_an_unknown_train_is_not_found() {
    let (mock, _) = hold_mock(true);
    let err = service(&mock)
        .hold_fare(HoldFareParams {
            departure_time: "10:05".into(),
            ..hold(Some(true))
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(mock.current(), 9, "nothing added to the basket");
}

// ---- A whole afternoon: later and earlier pages, coupled trains.

fn without_button(json: &str, name: &str) -> Node {
    fn strip(n: &mut Node, name: &str) {
        n.children
            .retain(|c| !(c.role == "button" && c.name == name));
        for c in &mut n.children {
            strip(c, name);
        }
    }
    let mut tree = parse(json);
    strip(&mut tree, name);
    tree
}

fn with_detail(mut page: Node, number: &str) -> Node {
    let detail = JOURNEY_DETAIL.replace("no. 6623", &format!("no. {number}"));
    fn doc(n: &mut Node) -> Option<&mut Node> {
        if n.role == "document web" && !n.name.is_empty() {
            return Some(n);
        }
        n.children.iter_mut().find_map(doc)
    }
    doc(&mut page).unwrap().children.push(parse(&detail));
    page
}

/// Search for 26 October from 2 PM, then results pages:
/// 9 first page, 10 after "next", 11 after another "next" (end of the day),
/// 12 after "previous", 13/15 with the details of the two 17:00 trains open,
/// 14 with the details closed again; 16-19 for a search from 5 PM.
fn afternoon_mock() -> MockDesktop {
    let next = "Display the next journeys";
    let previous = "Display previous journeys";
    let mut screens = search_screens();
    screens[7] = afternoon_mock_screen7();
    screens[9] = parse(RESULTS_2PM);
    screens.push(parse(RESULTS_2PM_NEXT)); // 10
    screens.push(without_button(RESULTS_2PM_NEXT, next)); // 11
    let all_day = without_button(RESULTS_2PM_PREVIOUS, next);
    screens.push(all_day.clone()); // 12
    screens.push(with_detail(all_day.clone(), "6623")); // 13
    screens.push(all_day.clone()); // 14
    screens.push(with_detail(all_day, "6687")); // 15
    // A search from 5 PM (fare hold): the 4 PM slot, then the details of
    // the 17:00 trains read on page 10.
    screens.push(parse(
        &TIME_CHOSEN
            .replace("Outbound journey time, 8 AM", "Outbound journey time, 4 PM")
            .replace(r#""value":"8 AM""#, r#""value":"4 PM""#)
            .replace("21/10/2026", "26/10/2026"),
    )); // 16
    let two_pages = parse(RESULTS_2PM_NEXT);
    screens.push(with_detail(two_pages.clone(), "6623")); // 17
    screens.push(two_pages.clone()); // 18
    screens.push(with_detail(two_pages, "6687")); // 19

    let click = |s: &str| Trigger::Click(s.to_owned());
    let typed = |s: &str| Trigger::Type(s.to_owned());
    MockDesktop::new(screens)
        .with_scrolling()
        .transition(Some(0), typed("Paris"), 1)
        .transition(Some(1), click("Paris : City"), 2)
        .transition(Some(2), typed("Lyon"), 3)
        .transition(Some(3), click("Lyon : City"), 4)
        .transition(Some(4), click("Outbound:"), 5)
        .transition(Some(5), click("Outbound journey time"), 6)
        .transition(Some(6), click("2 PM"), 7)
        .transition(Some(7), click("Confirm"), 8)
        .transition(Some(8), click("See prices"), 9)
        .transition(Some(9), click(next), 10)
        .transition(Some(10), click(next), 11)
        .transition(Some(11), click(previous), 12)
        .transition(Some(12), click("Route details"), 13)
        .transition(Some(13), click("Close the dialog box"), 14)
        .transition(Some(14), click("Route details"), 15)
        .transition(Some(15), click("Close the dialog box"), 14)
        .transition(Some(9), click(previous), 12)
        .transition(Some(6), click("4 PM"), 16)
        .transition(Some(16), click("Confirm"), 8)
        .transition(Some(10), click("Route details"), 17)
        .transition(Some(17), click("Close the dialog box"), 18)
        .transition(Some(18), click("Route details"), 19)
        .transition(Some(19), click("Close the dialog box"), 18)
}

/// The date dialog with 26 October and the 2 PM slot chosen.
fn afternoon_mock_screen7() -> Node {
    parse(
        &TIME_CHOSEN
            .replace("Outbound journey time, 8 AM", "Outbound journey time, 2 PM")
            .replace(r#""value":"8 AM""#, r#""value":"2 PM""#)
            .replace("21/10/2026", "26/10/2026"),
    )
}

fn afternoon() -> PricesParams {
    PricesParams {
        date: "2026-10-26".into(),
        time_after: Some("14:00".into()),
        ..paris_lyon()
    }
}

fn departures(res: &PricesResult) -> Vec<String> {
    res.journeys
        .iter()
        .map(|j| j.departure[11..16].to_owned())
        .collect()
}

#[tokio::test(start_paused = true)]
async fn prices_load_the_rest_of_the_day_and_earlier_trains() {
    let mock = Arc::new(afternoon_mock());
    let res = service(&mock).prices(afternoon()).await.unwrap();
    assert_eq!(typed(&mock), [SEARCH_URL, "Paris", "Lyon", "26/10/2026"]);
    assert_eq!(mock.current(), 14, "details closed again");
    assert_eq!(
        departures(&res),
        [
            "14:13", "14:56", "15:00", "15:35", "15:52", "16:49", "17:00", "17:00", "17:26",
            "17:27", "17:52", "18:26", "19:00", "19:26"
        ]
    );
    assert!(!res.more_later, "no more trains that day");
    assert_eq!(res.filtered_out, 0);

    // The two 17:00 are two trains (coupled units), with their own fares.
    let five: Vec<_> = res
        .journeys
        .iter()
        .filter(|j| j.departure == "2026-10-26T17:00:00+01:00")
        .collect();
    assert_eq!(five[0].train_numbers, ["6623"]);
    assert_eq!(five[0].price_first_class, Some(158.0));
    assert_eq!(five[1].train_numbers, ["6687"]);
    assert_eq!(five[1].price_first_class, Some(83.0));
    assert!(
        res.journeys
            .iter()
            .filter(|j| j.departure != "2026-10-26T17:00:00+01:00")
            .all(|j| j.train_numbers.is_empty()),
        "details are opened only for trains leaving together"
    );

    let cheapest = res.cheapest.as_ref().unwrap();
    assert_eq!(cheapest.departure, "2026-10-26T19:26:00+01:00");
    assert_eq!(cheapest.travel_class, TravelClass::Second);
    assert_eq!(cheapest.price, 49.0);
    assert_eq!(res.journeys[cheapest.index].departure, cheapest.departure);

    let clicked = mock.clicked();
    assert_eq!(
        clicked
            .iter()
            .filter(|n| *n == "Display the next journeys")
            .count(),
        2
    );
    assert!(
        !clicked.iter().any(|n| n.starts_with("Select your fare")),
        "{clicked:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn filters_apply_to_what_was_read() {
    let mock = Arc::new(afternoon_mock());
    let sncf = service(&mock);
    let all = sncf.prices(afternoon()).await.unwrap();
    let inputs = mock.inputs().len();

    let fast = sncf
        .prices(PricesParams {
            train_types: Some(vec![TrainType::TgvInoui, TrainType::Ouigo]),
            ..afternoon()
        })
        .await
        .unwrap();
    assert_eq!(mock.inputs().len(), inputs, "served from the cache");
    assert!(fast.journeys.iter().all(|j| matches!(
        j.train_category,
        Some(TrainType::TgvInoui | TrainType::Ouigo)
    )));
    assert_eq!(fast.filtered_out, 2, "the two TER");
    assert_eq!(fast.journeys.len() + 2, all.journeys.len());

    let direct = sncf
        .prices(PricesParams {
            direct_only: Some(true),
            ..afternoon()
        })
        .await
        .unwrap();
    assert!(
        direct
            .journeys
            .iter()
            .all(|j| j.direct && j.connections == 0)
    );
    assert_eq!(
        direct.journeys.len() + direct.filtered_out as usize,
        all.journeys.len()
    );

    let ter_only = sncf
        .prices(PricesParams {
            train_types: Some(vec![TrainType::Ter]),
            ..afternoon()
        })
        .await
        .unwrap();
    assert_eq!(departures(&ter_only), ["15:35", "17:27"]);
    assert_eq!(ter_only.cheapest.as_ref().unwrap().price, 65.6);
}

#[tokio::test(start_paused = true)]
async fn time_before_stops_loading_early() {
    let mock = Arc::new(afternoon_mock());
    let res = service(&mock)
        .prices(PricesParams {
            time_before: Some("16:00".into()),
            ..afternoon()
        })
        .await
        .unwrap();
    assert_eq!(
        departures(&res),
        ["14:13", "14:56", "15:00", "15:35", "15:52"]
    );
    assert_eq!(
        res.searched_until.as_deref(),
        Some("2026-10-26T16:00:00+01:00")
    );
    assert!(res.more_later);
    assert!(
        !mock
            .clicked()
            .iter()
            .any(|n| n == "Display the next journeys"),
        "the first page already goes past 16:00"
    );
}

#[tokio::test(start_paused = true)]
async fn a_list_that_does_not_grow_stops_the_loading() {
    // "next" and "previous" do nothing: no endless clicking.
    let mut screens = search_screens();
    screens[9] = parse(RESULTS_2PM);
    let click = |s: &str| Trigger::Click(s.to_owned());
    let typed = |s: &str| Trigger::Type(s.to_owned());
    screens[7] = afternoon_mock_screen7();
    let mock = Arc::new(
        MockDesktop::new(screens)
            .with_scrolling()
            .transition(Some(0), typed("Paris"), 1)
            .transition(Some(1), click("Paris : City"), 2)
            .transition(Some(2), typed("Lyon"), 3)
            .transition(Some(3), click("Lyon : City"), 4)
            .transition(Some(4), click("Outbound:"), 5)
            .transition(Some(5), click("Outbound journey time"), 6)
            .transition(Some(6), click("2 PM"), 7)
            .transition(Some(7), click("Confirm"), 8)
            .transition(Some(8), click("See prices"), 9),
    );
    let res = service(&mock).prices(afternoon()).await.unwrap();
    assert_eq!(res.journeys.len(), 7);
    assert!(res.more_later);
    let clicked = mock.clicked();
    let presses = clicked.iter().filter(|n| n.starts_with("Display")).count();
    assert_eq!(presses, 2, "one try each way: {clicked:?}");
}

#[tokio::test(start_paused = true)]
async fn holding_one_of_two_trains_leaving_together_needs_its_number() {
    let mock = Arc::new(afternoon_mock());
    let sncf = service(&mock);
    let params = |number: Option<&str>| HoldFareParams {
        date: "2026-10-26".into(),
        departure_time: "17:00".into(),
        travel_class: Some(TravelClass::First),
        train_number: number.map(String::from),
        ..hold(None)
    };
    let err = sncf.hold_fare(params(None)).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.to_string().contains("6623, 6687"), "{err}");
    let preview = sncf.hold_fare(params(Some("6687"))).await.unwrap();
    assert_eq!(preview.status, "preview");
    assert_eq!(preview.price, Some(83.0));
    assert_eq!(preview.journey.train_numbers, ["6687"]);
}
