//! Reading SNCF Connect pages from accessibility trees. Pure functions, so
//! they are tested on recorded trees.
//!
//! The site is used in English (`/en-en/` URLs); labels below are the
//! English accessible names.

use autocruise_core::time;
use autocruise_desktop::Node;
use chrono::{Duration, NaiveDate, NaiveTime};

use crate::types::{DayPrice, Journey, TravelClass};

pub(crate) const SEARCH_URL: &str = "https://www.sncf-connect.com/en-en/home/search/od";
pub(crate) const SEARCH_TITLE: &str = "SNCF Connect - Search";
pub(crate) const RESULTS_TITLE: &str = "SNCF Connect - Outbound journey itinerary results";
pub(crate) const DEPARTURE: &str = "Departure:";
pub(crate) const ARRIVAL: &str = "Arrival:";
pub(crate) const DATE_DIALOG: &str = "Select your outbound and return journey dates";
pub(crate) const DATE_ENTRY: &str = "*Outbound journey";
pub(crate) const TIME_COMBO: &str = "Outbound journey time";
pub(crate) const SERVICES_TITLE: &str = "SNCF Connect - Services";
pub(crate) const BASKET_TITLE: &str = "SNCF Connect - Basket";

/// Texts of bot checks and block pages (DataDome), English and French.
const CHALLENGE_MARKERS: &[&str] = &[
    "access is temporarily restricted",
    "accès temporairement restreint",
    "verification required",
    "vérification requise",
    "slide right",
    "glissez vers la droite",
    "captcha-delivery",
    "datadome",
    "please enable js and disable any ad blocker",
    "we want to make sure it is actually you",
    "nous souhaitons nous assurer qu'il s'agit bien de vous",
];

/// Replaces the narrow and non-breaking spaces SNCF Connect uses.
pub(crate) fn normalize(s: &str) -> String {
    s.replace(['\u{202f}', '\u{a0}'], " ").trim().to_owned()
}

/// The marker of a bot check or block page anywhere in the application.
pub(crate) fn challenge(app: &Node) -> Option<String> {
    app.iter().find_map(|n| {
        let texts = [
            n.name.to_lowercase(),
            n.value.clone().unwrap_or_default().to_lowercase(),
        ];
        CHALLENGE_MARKERS
            .iter()
            .find(|m| texts.iter().any(|t| t.contains(*m)))
            .map(|m| (*m).to_owned())
    })
}

/// The page shown in the active tab.
pub(crate) fn document(app: &Node) -> Option<&Node> {
    app.find(|n| n.role == "document web" && !n.name.is_empty())
}

/// Chrome's address bar.
pub(crate) fn address_bar(app: &Node) -> Option<&Node> {
    app.find(|n| n.role == "entry" && n.name == "Address and search bar")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Session {
    LoggedIn,
    LoggedOut,
    Unknown,
}

/// Login state from the site header.
pub(crate) fn session(doc: &Node) -> Session {
    if doc
        .find(|n| n.role == "link" && n.name == "My Account")
        .is_some()
    {
        Session::LoggedIn
    } else if doc
        .find(|n| matches!(n.role.as_str(), "button" | "link") && n.name.starts_with("Sign in"))
        .is_some()
    {
        Session::LoggedOut
    } else {
        Session::Unknown
    }
}

pub(crate) fn combo<'a>(doc: &'a Node, label: &str) -> Option<&'a Node> {
    doc.find(|n| n.role == "combo box" && n.name == label)
}

pub(crate) fn is_search_form(doc: &Node) -> bool {
    doc.name.starts_with(SEARCH_TITLE) && combo(doc, DEPARTURE).is_some()
}

pub(crate) fn button(doc: &Node, pred: impl Fn(&str) -> bool) -> Option<&Node> {
    doc.find(|n| n.role == "button" && pred(&n.name))
}

/// One entry of a station autocomplete list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Suggestion<'a> {
    pub node: &'a Node,
    pub name: String,
    /// "City", "Station", "Bus", "Metro"...
    pub kind: String,
}

/// Autocomplete entries of the list attached to a field (`label`).
pub(crate) fn suggestions<'a>(doc: &'a Node, label: &str) -> Vec<Suggestion<'a>> {
    let Some(list) = doc.find(|n| n.role == "list box" && n.name == label) else {
        return Vec::new();
    };
    list.children
        .iter()
        .filter(|n| n.role == "list item")
        .filter_map(|node| {
            let full = normalize(&node.name);
            let (name, rest) = full.split_once(" : ")?;
            let kind = rest.split(" , ").next().unwrap_or("").trim().to_owned();
            Some(Suggestion {
                node,
                name: name.trim().to_owned(),
                kind,
            })
        })
        .collect()
}

/// Display name of a suggestion list item ("Lyon : City , ..." -> "Lyon").
pub(crate) fn suggestion_name(item: &Node) -> String {
    let full = normalize(&item.name);
    full.split_once(" : ")
        .map_or(full.clone(), |(name, _)| name.trim().to_owned())
}

fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '-' | '\'' | '’' => ' ',
            c => c,
        })
        .collect()
}

/// Whether the list already reflects the typed query (and not the previous one).
pub(crate) fn suggestions_match(items: &[Suggestion<'_>], query: &str) -> bool {
    let q = fold(query);
    let first_word = q.split_whitespace().next().unwrap_or("");
    !items.is_empty() && items.iter().any(|s| fold(&s.name).contains(first_word))
}

/// Best entry for a query: exact name, then the first train city/station
/// containing the query, then the first city/station.
pub(crate) fn pick_suggestion<'a, 'b>(
    items: &'b [Suggestion<'a>],
    query: &str,
) -> Option<&'b Suggestion<'a>> {
    let q = fold(query);
    let rail = |s: &&Suggestion<'_>| matches!(s.kind.as_str(), "City" | "Station");
    items
        .iter()
        .filter(rail)
        .find(|s| fold(&s.name) == q)
        .or_else(|| {
            items
                .iter()
                .filter(rail)
                .find(|s| fold(&s.name).contains(&q))
        })
        .or_else(|| items.iter().find(rail))
}

pub(crate) fn parse_hour_label(label: &str) -> Option<u32> {
    let label = normalize(label);
    let (h, ampm) = label.split_once(' ')?;
    let h: u32 = h.parse().ok()?;
    match (ampm, h) {
        ("AM", 12) => Some(0),
        ("AM", 1..=11) => Some(h),
        ("PM", 12) => Some(12),
        ("PM", 1..=11) => Some(h + 12),
        _ => None,
    }
}

/// Index of the latest time slot starting at or before `hour` (else the
/// first slot).
pub(crate) fn pick_hour(options: &[String], hour: u32) -> Option<usize> {
    let hours: Vec<Option<u32>> = options.iter().map(|o| parse_hour_label(o)).collect();
    hours
        .iter()
        .enumerate()
        .filter(|(_, h)| h.is_some_and(|h| h <= hour))
        .max_by_key(|(_, h)| *h)
        .map(|(i, _)| i)
        .or_else(|| hours.iter().position(Option::is_some))
}

/// The time-slot list of the date dialog.
pub(crate) fn hour_options(doc: &Node) -> Vec<&Node> {
    doc.find(|n| n.role == "list box" && n.name == TIME_COMBO)
        .map(|l| {
            l.children
                .iter()
                .filter(|n| n.role == "list item")
                .collect()
        })
        .unwrap_or_default()
}

/// Everything read from a results page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Results {
    pub origin: String,
    pub destination: String,
    pub journeys: Vec<Journey>,
    pub days: Vec<DayPrice>,
    pub notices: Vec<String>,
    pub more_later: bool,
}

/// Whether the results page has finished loading.
pub(crate) fn results_ready(doc: &Node) -> bool {
    doc.name.starts_with(RESULTS_TITLE)
        && (button(doc, |n| n.starts_with("Select your fare")).is_some()
            || doc.contains_text("no results")
            || doc.contains_text("no journey")
            || doc.contains_text("no train"))
}

/// Parses a results page for `date`. Fails if the page is for another date.
pub(crate) fn parse_results(doc: &Node, date: NaiveDate) -> Result<Results, String> {
    let texts: Vec<String> = doc.texts().into_iter().map(normalize).collect();

    let day = format!("{} {}", date.format("%-d"), date.format("%B"));
    if !texts
        .iter()
        .any(|t| t.contains(&day) && t.contains("starting at"))
    {
        return Err(format!("the results are not for {day}"));
    }

    let (origin, destination) = texts
        .iter()
        .find_map(|t| t.strip_prefix("Outbound journey: "))
        .and_then(|od| od.split_once(" - "))
        .map(|(o, d)| (o.trim().to_owned(), d.trim().to_owned()))
        .unwrap_or_default();

    let journeys = doc
        .find_all(|n| {
            n.role == "list item"
                && n.children.iter().any(|c| {
                    c.find(|x| x.role == "heading" && x.name.contains("Departure from"))
                        .is_some()
                })
        })
        .into_iter()
        .filter_map(|item| parse_journey(item, date))
        .collect();

    let days = doc
        .find_all(|n| n.role == "toggle button" && n.name.starts_with("Results for "))
        .into_iter()
        .filter_map(|n| parse_day(&n.name))
        .collect();

    Ok(Results {
        origin,
        destination,
        journeys,
        days,
        notices: notices(&texts),
        more_later: button(doc, |n| n == "Display the next journeys").is_some(),
    })
}

fn notices(texts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(start) = texts.iter().position(|t| t == "Information on this route") else {
        return out;
    };
    for t in &texts[start + 1..] {
        if t == "List of results" || t == "Train" {
            break;
        }
        if t != "Warning" && t != "Information" && !out.contains(t) {
            out.push(t.clone());
        }
    }
    out
}

/// "Results for Wednesday 21 October 2026 from €75"
fn parse_day(name: &str) -> Option<DayPrice> {
    let rest = normalize(name);
    let rest = rest.strip_prefix("Results for ")?;
    let (date_part, price) = match rest.split_once(" from ") {
        Some((d, p)) => (d, parse_price(p)),
        None => (rest, None),
    };
    let mut words = date_part.split_whitespace();
    words.next()?; // weekday
    let date_str: Vec<&str> = words.collect();
    let date = NaiveDate::parse_from_str(&date_str.join(" "), "%d %B %Y").ok()?;
    Some(DayPrice {
        date: date.format("%Y-%m-%d").to_string(),
        cheapest_price: price,
    })
}

/// "€142.90", "€1,234", "75 €" -> amount.
pub(crate) fn parse_price(s: &str) -> Option<f64> {
    let s = normalize(s);
    if !s.contains('€') {
        return None;
    }
    let digits: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if digits.is_empty() || s.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    digits.parse().ok()
}

fn parse_clock(time: &str, ampm: Option<&str>) -> Option<NaiveTime> {
    match ampm {
        Some(ap) => NaiveTime::parse_from_str(&format!("{time} {ap}"), "%I:%M %p").ok(),
        None => NaiveTime::parse_from_str(time, "%H:%M").ok(),
    }
}

/// Splits "Paris - Gare De Lyon 10:14 AM ..." into place, time and the rest.
fn place_and_time(s: &str) -> Option<(String, NaiveTime)> {
    let words: Vec<&str> = s.split_whitespace().collect();
    let i = words.iter().position(|w| {
        w.len() >= 4 && w.contains(':') && w.chars().all(|c| c.is_ascii_digit() || c == ':')
    })?;
    let ampm = words
        .get(i + 1)
        .copied()
        .filter(|w| *w == "AM" || *w == "PM");
    let time = parse_clock(words[i], ampm)?;
    Some((words[..i].join(" "), time))
}

/// "TGV INOUI Departure from Paris - Gare De Lyon 10:14 AM bound for Grenoble 1:13 PM"
pub(crate) fn parse_heading(name: &str) -> Option<(String, String, NaiveTime, String, NaiveTime)> {
    let s = normalize(name);
    let (label, rest) = s.split_once("Departure from ")?;
    let (left, right) = rest.split_once(" bound for ")?;
    let (origin, dep) = place_and_time(left)?;
    let (destination, arr) = place_and_time(right)?;
    Some((label.trim().to_owned(), origin, dep, destination, arr))
}

fn parse_duration(s: &str) -> Option<u32> {
    let s = s.strip_prefix("Duration: ")?;
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut minutes = 0;
    for pair in words.chunks(2) {
        let n: u32 = pair.first()?.parse().ok()?;
        match pair.get(1).copied()? {
            "hour" | "hours" => minutes += n * 60,
            "minute" | "minutes" => minutes += n,
            _ => return None,
        }
    }
    Some(minutes)
}

fn parse_seats(s: &str) -> Option<u32> {
    let (n, rest) = s.split_once(' ')?;
    if rest == "seats at this price" || rest == "seat at this price" {
        n.parse().ok()
    } else {
        None
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    None,
    Second,
    First,
}

fn parse_journey(item: &Node, date: NaiveDate) -> Option<Journey> {
    let heading = item.find(|n| n.role == "heading" && n.name.contains("Departure from"))?;
    let (label, origin, dep, destination, arr) = parse_heading(&heading.name)?;
    let texts: Vec<String> = item.texts().into_iter().map(normalize).collect();

    let duration = texts.iter().find_map(|t| parse_duration(t));
    let direct = texts.iter().any(|t| t == "Direct");
    let connections = texts
        .iter()
        .find_map(|t| {
            let (n, rest) = t.split_once(' ')?;
            matches!(rest, "connection" | "connections")
                .then(|| n.parse().ok())
                .flatten()
        })
        .unwrap_or(0);

    let mut class = Class::None;
    let (mut second, mut first) = (None, None);
    let (mut seats_second, mut seats_first) = (None, None);
    for t in &texts {
        match t.as_str() {
            "Second class" => class = Class::Second,
            "First class" => class = Class::First,
            _ => {
                if let Some(p) = parse_price(t) {
                    match class {
                        Class::Second if second.is_none() => second = Some(p),
                        Class::First if first.is_none() => first = Some(p),
                        _ => {}
                    }
                } else if let Some(n) = parse_seats(t) {
                    match class {
                        Class::Second => seats_second = Some(n),
                        Class::First => seats_first = Some(n),
                        Class::None => {}
                    }
                }
            }
        }
    }

    let departure = time::localize(date.and_time(dep)).ok()?;
    let mut arrival_naive = date.and_time(arr);
    if let Some(minutes) = duration {
        // The expected arrival tells the day (night trains).
        let expected = date.and_time(dep) + Duration::minutes(i64::from(minutes));
        arrival_naive = expected.date().and_time(arr);
    } else if arr < dep {
        arrival_naive += Duration::days(1);
    }
    let arrival = time::localize(arrival_naive).ok()?;

    let is_connection =
        label.is_empty() || label == "Trajet en correspondance" || label == "Connecting journey";
    Some(Journey {
        departure: time::format(&departure),
        arrival: time::format(&arrival),
        duration_minutes: duration,
        origin_station: origin,
        destination_station: destination,
        train_type: (!is_connection).then_some(label),
        direct: direct || (connections == 0 && !is_connection),
        connections,
        price_second_class: second,
        price_first_class: first,
        seats_left_second_class: seats_second,
        seats_left_first_class: seats_first,
        fully_booked: texts.iter().any(|t| t == "Fully booked"),
    })
}

/// Whether pressing this control could pay. autocruise never presses it.
pub(crate) fn is_payment(name: &str) -> bool {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| {
            matches!(
                w,
                "pay" | "payer" | "payez" | "payment" | "paiement" | "purchase" | "buy"
            )
        })
}

/// Results list item of the train leaving at `departure`.
pub(crate) fn journey_item(doc: &Node, departure: NaiveTime) -> Option<&Node> {
    doc.find(|n| {
        n.role == "list item"
            && n.children.iter().any(|c| {
                c.find(|x| {
                    x.role == "heading"
                        && parse_heading(&x.name).is_some_and(|(_, _, dep, _, _)| dep == departure)
                })
                .is_some()
            })
    })
}

/// The "Select your fare" button of a results item.
pub(crate) fn fare_toggle(item: &Node) -> Option<&Node> {
    item.find(|n| n.role == "button" && n.name.starts_with("Select your fare"))
}

/// The "Choose this fare" button of the standard fare of a class, with its
/// price, in an expanded results item.
pub(crate) fn fare_choice(item: &Node, class: TravelClass) -> Option<(Node, Option<f64>)> {
    let wanted = |heading: &str| match class {
        TravelClass::Second => heading.starts_with("Second Class :"),
        TravelClass::First => {
            heading.starts_with("1st Class :") || heading.starts_with("First Class :")
        }
    };
    let mut current: Option<String> = None;
    for n in item.iter() {
        if n.role == "heading" && n.name.contains(" : €") {
            current = Some(normalize(&n.name));
        } else if n.role == "button"
            && n.name == "Choose this fare"
            && let Some(h) = current.as_deref().filter(|h| wanted(h))
        {
            let price = h
                .split_once(" : ")
                .and_then(|(_, rest)| parse_price(rest.split(',').next().unwrap_or("")));
            return Some((n.clone(), price));
        }
    }
    None
}

/// Free option ("Place an option for free") state on the basket page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HoldOffer {
    /// SNCF Connect says no (its reason).
    Refused(String),
    /// The control that starts the option.
    Offered(Node),
    Unknown,
}

pub(crate) fn hold_offer(doc: &Node) -> HoldOffer {
    if let Some(reason) = doc
        .iter()
        .map(|n| normalize(&n.name))
        .find(|t| t.starts_with("Option not applicable") || t.starts_with("Option not available"))
    {
        return HoldOffer::Refused(reason);
    }
    doc.find(|n| {
        matches!(
            n.role.as_str(),
            "button" | "radio button" | "check box" | "link"
        ) && n.name.to_lowercase().contains("place an option")
            && !n.name.starts_with("More information")
    })
    .map_or(HoldOffer::Unknown, |n| HoldOffer::Offered(n.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_controls() {
        assert!(is_payment("Confirm and pay"));
        assert!(is_payment("Payer"));
        assert!(!is_payment("Display the next journeys"));
        assert!(!is_payment("Place an option for free"));
        assert!(!is_payment("Confirm your option"));
    }

    #[test]
    fn headings() {
        let (label, from, dep, to, arr) = parse_heading(
            "TGV INOUI Departure from Paris - Gare De Lyon - Hall 1 & 2 10:14\u{202f}AM bound for Grenoble 1:13\u{202f}PM",
        )
        .unwrap();
        assert_eq!(label, "TGV INOUI");
        assert_eq!(from, "Paris - Gare De Lyon - Hall 1 & 2");
        assert_eq!(dep, NaiveTime::from_hms_opt(10, 14, 0).unwrap());
        assert_eq!(to, "Grenoble");
        assert_eq!(arr, NaiveTime::from_hms_opt(13, 13, 0).unwrap());
    }

    #[test]
    fn prices_durations_seats() {
        assert_eq!(parse_price("€142.90"), Some(142.9));
        assert_eq!(parse_price("€1,234.50"), Some(1234.5));
        assert_eq!(parse_price("75 €"), Some(75.0));
        assert_eq!(parse_price("From"), None);
        assert_eq!(parse_price("2 seats at this price"), None);
        assert_eq!(parse_duration("Duration: 2 hours 59 minutes"), Some(179));
        assert_eq!(parse_duration("Duration: 1 hour"), Some(60));
        assert_eq!(parse_duration("Duration: 45 minutes"), Some(45));
        assert_eq!(parse_seats("1 seat at this price"), Some(1));
        assert_eq!(parse_seats("9 seats at this price"), Some(9));
    }

    #[test]
    fn days() {
        assert_eq!(
            parse_day("Results for Wednesday 21 October 2026 from €75"),
            Some(DayPrice {
                date: "2026-10-21".into(),
                cheapest_price: Some(75.0)
            })
        );
        assert_eq!(
            parse_day("Results for Tuesday 20 October 2026")
                .unwrap()
                .cheapest_price,
            None
        );
    }

    #[test]
    fn hours() {
        assert_eq!(parse_hour_label("12 AM"), Some(0));
        assert_eq!(parse_hour_label("6\u{a0}AM"), Some(6));
        assert_eq!(parse_hour_label("12 PM"), Some(12));
        assert_eq!(parse_hour_label("2 PM"), Some(14));
        let options: Vec<String> = ["4 AM", "6 AM", "8 AM", "10 AM", "12 PM", "2 PM"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(pick_hour(&options, 9), Some(2));
        assert_eq!(pick_hour(&options, 13), Some(4));
        assert_eq!(pick_hour(&options, 2), Some(0));
        assert_eq!(pick_hour(&options, 23), Some(5));
    }

    #[test]
    fn suggestion_choice() {
        let n = Node::new("list item", "x");
        let s = |name: &str, kind: &str| Suggestion {
            node: &n,
            name: name.into(),
            kind: kind.into(),
        };
        let items = vec![
            s("Paris Gare de Lyon", "Station"),
            s("Lyon", "City"),
            s("Lyon Part-Dieu", "Station"),
            s("Lyon", "Bus"),
        ];
        assert_eq!(pick_suggestion(&items, "lyon").unwrap().name, "Lyon");
        assert_eq!(
            pick_suggestion(&items, "Lyon Part Dieu").unwrap().name,
            "Lyon Part-Dieu"
        );
        assert!(suggestions_match(&items, "Lyon"));
        assert!(!suggestions_match(&items, "Grenoble"));
    }

    #[test]
    fn challenge_markers() {
        let blocked = Node::new("application", "Google Chrome").with_children(vec![
            Node::new("document web", "sncf-connect.com").with_children(vec![Node::new(
                "static",
                "Access is temporarily restricted",
            )]),
        ]);
        assert_eq!(
            challenge(&blocked).as_deref(),
            Some("access is temporarily restricted")
        );
        assert_eq!(challenge(&Node::new("document web", "SNCF Connect")), None);
    }
}
