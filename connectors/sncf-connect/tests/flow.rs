//! End-to-end flows against a mock desktop: recorded (sanitized)
//! accessibility trees of sncf-connect.com, with transitions on inputs.
//! Nothing here touches a display or the network.

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::error::{Classify, ErrorKind};
use autocruise_desktop::mock::{MockDesktop, Trigger};
use autocruise_desktop::{Input, Node, Rect};
use autocruise_sncf_connect::types::{HoldFareParams, PricesParams, TravelClass};
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
        passengers: None,
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
