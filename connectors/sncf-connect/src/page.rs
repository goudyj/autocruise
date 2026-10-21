//! Reading SNCF Connect pages from accessibility trees. Pure functions, so
//! they are tested on recorded trees.
//!
//! The site is used in English (`/en-en/` URLs); labels below are the
//! English accessible names.

use autocruise_core::time;
use autocruise_desktop::Node;
use chrono::{Duration, NaiveDate, NaiveTime};

use crate::types::{DayPrice, Journey, TrainType, TravelClass};

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

/// Texts of bot checks and block pages (DataDome), English and French, as
/// shown to the user in the page. Bare "datadome" is not a marker: the word
/// shows up outside challenges (extensions, history, third-party scripts).
const CHALLENGE_MARKERS: &[&str] = &[
    "access is temporarily restricted",
    "accès temporairement restreint",
    "verification required",
    "vérification requise",
    "slide right",
    "glissez vers la droite",
    "please enable js and disable any ad blocker",
    "we want to make sure it is actually you",
    "nous souhaitons nous assurer qu'il s'agit bien de vous",
];

/// Names of DataDome's iframes. DataDome also runs an invisible
/// "Device Check" frame in the background on normal pages, so these only
/// count when the frame is actually shown with a usable size.
const CHALLENGE_FRAME_MARKERS: &[&str] = &[
    "captcha-delivery",
    "datadome captcha",
    "datadome device check",
];

/// Smallest on-screen frame taken as a shown check (DataDome's visible
/// checks are several hundred pixels wide; background frames are 0x0 or tiny).
const MIN_CHALLENGE_FRAME: (i32, i32) = (150, 100);

fn frame_is_shown(n: &Node) -> bool {
    n.bounds
        .is_some_and(|b| b.width >= MIN_CHALLENGE_FRAME.0 && b.height >= MIN_CHALLENGE_FRAME.1)
        && (n.states.is_empty() || n.has_state("showing"))
}

/// Replaces the narrow and non-breaking spaces SNCF Connect uses.
pub(crate) fn normalize(s: &str) -> String {
    s.replace(['\u{202f}', '\u{a0}'], " ").trim().to_owned()
}

/// The marker of a bot check or block page in the page shown in the active
/// tab, including its iframes (the DataDome check is an iframe from
/// captcha-delivery.com). Other tabs, the address bar, extension popups and
/// ad frames are not looked at.
pub(crate) fn challenge(app: &Node) -> Option<String> {
    let doc = active_document(app)?;
    let mut stack = vec![doc];
    while let Some(n) = stack.pop() {
        let texts = [
            n.name.to_lowercase(),
            n.value.clone().unwrap_or_default().to_lowercase(),
        ];
        if let Some(m) = CHALLENGE_MARKERS
            .iter()
            .find(|m| texts.iter().any(|t| t.contains(*m)))
        {
            return Some((*m).to_owned());
        }
        if let Some(m) = CHALLENGE_FRAME_MARKERS
            .iter()
            .find(|m| texts.iter().any(|t| t.contains(*m)))
            && frame_is_shown(n)
        {
            return Some((*m).to_owned());
        }
        stack.extend(
            n.children
                .iter()
                .filter(|c| !(c.role == "internal frame" && c.name == "Advertisement")),
        );
    }
    None
}

/// Top-level documents (not inside another document): the pages of tabs,
/// extension popups, Chrome's own web UI.
fn top_documents(app: &Node) -> Vec<&Node> {
    let mut out = Vec::new();
    let mut stack = vec![app];
    while let Some(n) = stack.pop() {
        if n.role == "document web" {
            out.push(n);
        } else {
            stack.extend(n.children.iter().rev());
        }
    }
    out
}

/// The page of the active tab: the largest top-level document on screen
/// (Chrome keeps hidden tabs out of `showing`; popups are small).
pub(crate) fn active_document(app: &Node) -> Option<&Node> {
    let docs = top_documents(app);
    let showing: Vec<&Node> = docs
        .iter()
        .copied()
        .filter(|d| d.has_state("showing"))
        .collect();
    let pool = if showing.is_empty() { docs } else { showing };
    let area = |d: &Node| {
        d.bounds
            .filter(|b| !b.is_empty())
            .map_or(0, |b| i64::from(b.width) * i64::from(b.height))
    };
    // max_by_key keeps the last maximum: reverse so ties go to the first.
    pool.into_iter().rev().max_by_key(|d| area(d))
}

/// The page shown in the active tab, once it has a title.
pub(crate) fn document(app: &Node) -> Option<&Node> {
    active_document(app).filter(|d| !d.name.is_empty())
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
    /// Journeys of the requested day, in page (departure) order.
    pub journeys: Vec<Journey>,
    /// For each journey, its index in [`journey_cards`].
    pub cards: Vec<usize>,
    pub days: Vec<DayPrice>,
    pub notices: Vec<String>,
    /// "Display the next journeys" is shown.
    pub next_button: bool,
    /// "Display previous journeys" is shown.
    pub previous_button: bool,
    /// Cards for the following day were loaded: the requested day is over.
    pub day_over: bool,
    /// Cards for the previous day were loaded: nothing earlier that day.
    pub day_started: bool,
}

pub(crate) const NEXT_JOURNEYS: &str = "Display the next journeys";
pub(crate) const PREVIOUS_JOURNEYS: &str = "Display previous journeys";

/// Result cards (one per offer), in page order.
pub(crate) fn journey_cards(doc: &Node) -> Vec<&Node> {
    doc.find_all(|n| {
        n.role == "list item"
            && n.children.iter().any(|c| {
                c.find(|x| x.role == "heading" && x.name.contains("Departure from"))
                    .is_some()
            })
    })
}

/// Departure time shown on a card.
fn card_departure(card: &Node) -> Option<NaiveTime> {
    card.find(|n| n.role == "heading" && n.name.contains("Departure from"))
        .and_then(|h| parse_heading(&h.name))
        .map(|(_, _, dep, _, _)| dep)
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
///
/// Loading next or previous journeys can run past midnight: the list then
/// goes back in time (23:30, then 05:50). Cards are split into days at such
/// drops; the day holding the card that leaves at `anchor` (the first train
/// of the first page) is `date`, else the first one.
pub(crate) fn parse_results(
    doc: &Node,
    date: NaiveDate,
    anchor: Option<NaiveTime>,
) -> Result<Results, String> {
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

    let cards = journey_cards(doc);
    let times: Vec<Option<NaiveTime>> = cards.iter().map(|c| card_departure(c)).collect();
    let mut segments = Vec::with_capacity(times.len());
    let (mut segment, mut last): (i64, Option<NaiveTime>) = (0, None);
    for t in &times {
        if let (Some(t), Some(l)) = (t, last) {
            // NaiveTime arithmetic wraps: compare the signed gap.
            if l.signed_duration_since(*t) > Duration::hours(6) {
                segment += 1;
            }
        }
        if t.is_some() {
            last = *t;
        }
        segments.push(segment);
    }
    let base = anchor
        .and_then(|a| times.iter().position(|t| *t == Some(a)))
        .map_or(0, |i| segments[i]);
    let mut journeys = Vec::new();
    let mut indices = Vec::new();
    for (i, card) in cards.iter().enumerate() {
        if segments[i] != base {
            continue;
        }
        if let Some(j) = parse_journey(card, date) {
            journeys.push(j);
            indices.push(i);
        }
    }
    let day_over = segments.iter().any(|s| *s > base);
    let day_started = segments.iter().any(|s| *s < base);

    let days = doc
        .find_all(|n| n.role == "toggle button" && n.name.starts_with("Results for "))
        .into_iter()
        .filter_map(|n| parse_day(&n.name))
        .collect();

    Ok(Results {
        origin,
        destination,
        journeys,
        cards: indices,
        days,
        notices: notices(&texts),
        next_button: button(doc, |n| n == NEXT_JOURNEYS).is_some(),
        previous_button: button(doc, |n| n == PREVIOUS_JOURNEYS).is_some(),
        day_over,
        day_started,
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

/// "Train T E R" (spelled out for screen readers) -> "TER".
pub(crate) fn clean_label(label: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut letters = String::new();
    for w in label.split_whitespace() {
        if w.len() == 1 && w.chars().all(|c| c.is_ascii_uppercase()) {
            letters.push_str(w);
        } else {
            if !letters.is_empty() {
                words.push(std::mem::take(&mut letters));
            }
            words.push(w.to_owned());
        }
    }
    if !letters.is_empty() {
        words.push(letters);
    }
    if words.len() == 2
        && words[0] == "Train"
        && words[1].len() > 1
        && words[1].chars().all(|c| c.is_ascii_uppercase())
    {
        words.remove(0);
    }
    words.join(" ")
}

/// Category of a cleaned train label.
pub(crate) fn train_category(label: &str) -> TrainType {
    let l = fold(label);
    let words: Vec<&str> = l.split_whitespace().collect();
    if l.contains("inoui") {
        TrainType::TgvInoui
    } else if l.contains("ouigo") && l.contains("classique") {
        TrainType::OuigoClassique
    } else if l.contains("ouigo") {
        TrainType::Ouigo
    } else if l.contains("lyria") {
        TrainType::Lyria
    } else if l.contains("eurostar") {
        TrainType::Eurostar
    } else if l.contains("intercit") {
        TrainType::Intercites
    } else if words.contains(&"ter") {
        TrainType::Ter
    } else {
        TrainType::Other
    }
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
    let label = clean_label(&label);
    Some(Journey {
        departure: time::format(&departure),
        arrival: time::format(&arrival),
        duration_minutes: duration,
        origin_station: origin,
        destination_station: destination,
        train_category: (!is_connection).then(|| train_category(&label)),
        train_type: (!is_connection).then_some(label),
        train_numbers: Vec::new(),
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

/// The `nth` (0-based) results card of a train leaving at `departure`.
pub(crate) fn journey_item(doc: &Node, departure: NaiveTime, nth: usize) -> Option<&Node> {
    journey_cards(doc)
        .into_iter()
        .filter(|c| card_departure(c) == Some(departure))
        .nth(nth)
}

/// The "Route details" button of a card (opens "This journey in detail").
pub(crate) fn route_details(card: &Node) -> Option<&Node> {
    card.find(|n| n.role == "button" && n.name.starts_with("Route details"))
}

pub(crate) const DETAIL_DIALOG: &str = "This journey in detail";
pub(crate) const DETAIL_CLOSE: &str = "Close the dialog box and return to the current page";

pub(crate) fn detail_dialog(doc: &Node) -> Option<&Node> {
    doc.find(|n| n.role == "dialog" && n.name == DETAIL_DIALOG)
}

/// Train numbers in a journey detail ("no. 6623"), in travel order.
pub(crate) fn train_numbers(dialog: &Node) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in dialog.iter().filter(|n| n.role == "static") {
        let t = normalize(&n.name);
        if let Some(num) = t.strip_prefix("no. ").or_else(|| t.strip_prefix("n° "))
            && !num.is_empty()
            && num.chars().all(|c| c.is_ascii_alphanumeric())
            && !out.iter().any(|o| o == num)
        {
            out.push(num.to_owned());
        }
    }
    out
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
    use autocruise_desktop::Rect;

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

    const RESULTS: &str = include_str!("../tests/fixtures/results.json");
    const RESULTS_2PM: &str = include_str!("../tests/fixtures/results_2pm.json");
    const RESULTS_2PM_NEXT: &str = include_str!("../tests/fixtures/results_2pm_next.json");
    const RESULTS_2PM_PREVIOUS: &str = include_str!("../tests/fixtures/results_2pm_previous.json");
    const JOURNEY_DETAIL: &str = include_str!("../tests/fixtures/journey_detail.json");

    fn tree(json: &str) -> Node {
        serde_json::from_str(json).expect("fixture")
    }

    fn active_doc_mut(app: &mut Node) -> &mut Node {
        fn walk(n: &mut Node) -> Option<&mut Node> {
            if n.role == "document web" && n.has_state("showing") && !n.name.is_empty() {
                return Some(n);
            }
            n.children.iter_mut().find_map(walk)
        }
        walk(app).expect("active document")
    }

    fn oct26() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
    }

    #[test]
    fn markers_outside_the_active_document_are_not_a_challenge() {
        let mut app = tree(RESULTS);
        let frame = &mut app.children[0];
        // Another tab, the address bar, an extension popup, history.
        frame.children.push(
            Node::new("page tab", "DataDome CAPTCHA - captcha-delivery.com")
                .with_bounds(Rect::new(290, 0, 256, 41)),
        );
        frame.children.push(Node {
            value: Some("geo.captcha-delivery.com/captcha/?initialCid=x".into()),
            ..Node::new("entry", "Address and search bar")
        });
        frame.children.push(
            Node::new("document web", "Bitwarden")
                .with_bounds(Rect::new(1500, 90, 380, 600))
                .with_children(vec![Node::new("static", "Verification required")]),
        );
        // A background tab still in the tree, not shown.
        frame.children.push(
            Node::new("document web", "History")
                .with_bounds(Rect::new(0, 87, 1919, 992))
                .with_children(vec![Node::new(
                    "link",
                    "Access is temporarily restricted - sncf-connect.com",
                )]),
        );
        // The word alone, in the page: a script or a cookie banner.
        active_doc_mut(&mut app)
            .children
            .push(Node::new("static", "Protected by DataDome"));
        // An ad frame inside the page.
        active_doc_mut(&mut app).children.push(
            Node::new("internal frame", "Advertisement")
                .with_children(vec![Node::new("static", "Slide right to see our offers")]),
        );
        assert_eq!(challenge(&app), None);
        assert_eq!(
            document(&app).map(|d| d.name.as_str()),
            Some(RESULTS_TITLE),
            "the active page is still found"
        );
    }

    #[test]
    fn a_captcha_iframe_inside_the_active_document_is_a_challenge() {
        let mut app = tree(RESULTS);
        active_doc_mut(&mut app).children.push(
            Node::new("internal frame", "DataDome CAPTCHA")
                .with_bounds(Rect::new(560, 300, 800, 500))
                .with_children(vec![Node::new("document web", "")]),
        );
        assert_eq!(challenge(&app).as_deref(), Some("datadome captcha"));

        let mut app = tree(RESULTS);
        active_doc_mut(&mut app)
            .children
            .push(Node::new("internal frame", "").with_children(vec![
                Node::new("document web", "").with_children(vec![Node::new(
                    "static",
                    "Slide right to secure your access",
                )]),
            ]));
        assert_eq!(challenge(&app).as_deref(), Some("slide right"));
    }

    #[test]
    fn a_background_device_check_frame_is_not_a_challenge() {
        // DataDome's invisible device check, as seen on normal pages.
        for bounds in [
            None,
            Some(Rect::new(0, 0, 0, 0)),
            Some(Rect::new(10, 10, 1, 1)),
        ] {
            let mut app = tree(RESULTS);
            let mut frame = Node::new("internal frame", "DataDome Device Check")
                .with_children(vec![Node::new("document web", "")]);
            frame.bounds = bounds;
            active_doc_mut(&mut app).children.push(frame);
            assert_eq!(challenge(&app), None, "{bounds:?}");
        }
        // The same frame shown in the middle of the page is a check.
        let mut app = tree(RESULTS);
        active_doc_mut(&mut app).children.push(
            Node::new("internal frame", "DataDome Device Check")
                .with_bounds(Rect::new(560, 300, 800, 500)),
        );
        assert_eq!(challenge(&app).as_deref(), Some("datadome device check"));
    }

    #[test]
    fn active_document_ignores_popups() {
        let app = tree(RESULTS_2PM);
        // The recorded tree also has a focused, empty, 0x0 document (Chrome UI).
        assert_eq!(document(&app).unwrap().name, RESULTS_TITLE);
    }

    #[test]
    fn two_trains_leaving_together_are_two_offers() {
        let r = parse_results(&tree(RESULTS_2PM), oct26(), None).unwrap();
        let five: Vec<&Journey> = r
            .journeys
            .iter()
            .filter(|j| j.departure == "2026-10-26T17:00:00+01:00")
            .collect();
        assert_eq!(five.len(), 2, "two cards on the page");
        assert_eq!(
            (five[0].price_second_class, five[0].price_first_class),
            (Some(70.0), Some(158.0))
        );
        assert_eq!(
            (five[1].price_second_class, five[1].price_first_class),
            (Some(70.0), Some(83.0))
        );
        assert_eq!(five[1].seats_left_first_class, Some(4));
        assert_ne!(r.cards[5], r.cards[6], "different cards");
        // Their details tell them apart (TGV INOUI 6623 and 6687).
        let detail = tree(JOURNEY_DETAIL);
        assert_eq!(train_numbers(&detail), ["6623"]);
        assert!(r.next_button && r.previous_button && !r.day_over);
    }

    #[test]
    fn later_pages_extend_the_list() {
        let first = parse_results(&tree(RESULTS_2PM), oct26(), None).unwrap();
        assert_eq!(first.journeys.len(), 7);
        let anchor = Some(NaiveTime::from_hms_opt(14, 56, 0).unwrap());
        let next = parse_results(&tree(RESULTS_2PM_NEXT), oct26(), anchor).unwrap();
        assert_eq!(next.journeys.len(), 13);
        assert_eq!(next.journeys[..7], first.journeys[..]);
        assert_eq!(
            next.journeys.last().unwrap().departure,
            "2026-10-26T19:26:00+01:00"
        );
        let previous = parse_results(&tree(RESULTS_2PM_PREVIOUS), oct26(), anchor).unwrap();
        assert_eq!(previous.journeys.len(), 21);
        let ouigo = previous
            .journeys
            .iter()
            .find(|j| j.departure == "2026-10-26T14:13:00+01:00")
            .expect("the 14:13 is only behind \"previous\"");
        assert_eq!(ouigo.train_category, Some(TrainType::Ouigo));
        assert_eq!(ouigo.origin_station, "Marne-La-Vallee - Chessy");
        let ter = previous
            .journeys
            .iter()
            .find(|j| j.departure == "2026-10-26T15:35:00+01:00")
            .unwrap();
        assert_eq!(ter.train_type.as_deref(), Some("TER"));
        assert_eq!(ter.train_category, Some(TrainType::Ter));
    }

    #[test]
    fn cards_past_midnight_belong_to_the_next_day() {
        let json = RESULTS_2PM_NEXT.replace(
            "7:26\u{202f}PM bound for Lyon Part Dieu 9:22\u{202f}PM",
            "5:50\u{202f}AM bound for Lyon Part Dieu 7:46\u{202f}AM",
        );
        assert_ne!(json, RESULTS_2PM_NEXT);
        let anchor = Some(NaiveTime::from_hms_opt(14, 56, 0).unwrap());
        let r = parse_results(&tree(&json), oct26(), anchor).unwrap();
        assert_eq!(r.journeys.len(), 12, "the 5:50 AM is the next morning");
        assert!(r.day_over && !r.day_started);
    }

    #[test]
    fn labels_and_categories() {
        assert_eq!(clean_label("Train T E R"), "TER");
        assert_eq!(clean_label("TGV INOUI"), "TGV INOUI");
        assert_eq!(clean_label("OUIGO Grande Vitesse"), "OUIGO Grande Vitesse");
        assert_eq!(train_category("TGV INOUI"), TrainType::TgvInoui);
        assert_eq!(train_category("OUIGO Grande Vitesse"), TrainType::Ouigo);
        assert_eq!(
            train_category("OUIGO Train Classique"),
            TrainType::OuigoClassique
        );
        assert_eq!(train_category("TER"), TrainType::Ter);
        assert_eq!(train_category("Intercités de nuit"), TrainType::Intercites);
        assert_eq!(train_category("TGV Lyria"), TrainType::Lyria);
        assert_eq!(train_category("Frecciarossa"), TrainType::Other);
    }
}
