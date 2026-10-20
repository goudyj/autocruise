//! The SNCF Connect service: drives the user's browser, one search at a time.

use std::sync::Arc;
use std::time::Duration;

use autocruise_core::cache::TtlCache;
use autocruise_core::throttle::Throttle;
use autocruise_core::time;
use autocruise_desktop::linux::LinuxDesktop;
use autocruise_desktop::{Check, Desktop, DesktopError, Driver, Node, Rect};
use chrono::{NaiveDate, NaiveTime, Timelike};
use tokio::sync::Mutex;

use crate::error::SncfConnectError;
use crate::page::{self, Session};
use crate::types::{
    HoldFareParams, HoldFareResult, Journey, PricesParams, PricesResult, TravelClass,
};

/// Application name on the accessibility bus.
const APP: &str = "chrome";

/// Pacing and patience. Defaults are human-paced.
#[derive(Debug, Clone)]
pub struct Timing {
    /// Minimum delay between two inputs (clicks, key presses, typed strings).
    pub input_interval: Duration,
    /// Minimum delay between two searches on the site.
    pub search_interval: Duration,
    /// Delay between two reads of the page while waiting.
    pub poll: Duration,
    /// How long to wait for a page to load.
    pub page_timeout: Duration,
    /// How long results are reused.
    pub cache_ttl: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            input_interval: Duration::from_millis(450),
            search_interval: Duration::from_secs(30),
            poll: Duration::from_millis(700),
            page_timeout: Duration::from_secs(45),
            cache_ttl: Duration::from_secs(5 * 60),
        }
    }
}

/// The SNCF Connect connector. Cheap to share behind an `Arc`.
#[derive(Debug)]
pub struct SncfConnect {
    driver: Driver,
    timing: Timing,
    cache: TtlCache<PricesResult>,
    /// One browser session: one search at a time.
    busy: Mutex<()>,
    searches: Throttle,
    /// Fixed "today" (tests); the real date otherwise.
    today: Option<NaiveDate>,
}

/// A validated price request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    from: String,
    to: String,
    date: NaiveDate,
    time_after: Option<NaiveTime>,
    passengers: u8,
}

impl Request {
    fn parse(p: PricesParams, today: NaiveDate) -> Result<Self, SncfConnectError> {
        let place = |s: &str, what: &str| {
            let s = s.trim();
            if s.is_empty() || s.chars().count() > 80 {
                Err(SncfConnectError::Invalid(format!(
                    "{what} must be a city or station name"
                )))
            } else {
                Ok(s.to_owned())
            }
        };
        let date = time::parse_date(&p.date)?;
        if date < today {
            return Err(SncfConnectError::Invalid(format!("{date} is in the past")));
        }
        let time_after = p.time_after.as_deref().map(parse_time).transpose()?;
        let passengers = p.passengers.unwrap_or(1);
        if !(1..=8).contains(&passengers) {
            return Err(SncfConnectError::Invalid(
                "passengers must be between 1 and 8".into(),
            ));
        }
        Ok(Self {
            from: place(&p.from, "from")?,
            to: place(&p.to, "to")?,
            date,
            time_after,
            passengers,
        })
    }

    fn cache_key(&self) -> String {
        format!(
            "{}|{}|{}|{:?}|{}",
            self.from.to_lowercase(),
            self.to.to_lowercase(),
            self.date,
            self.time_after,
            self.passengers
        )
    }
}

fn parse_time(s: &str) -> Result<NaiveTime, SncfConnectError> {
    let s = s.trim();
    NaiveTime::parse_from_str(s, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
        .map_err(|_| ())
        .or_else(|()| time::parse_datetime(s).map(|dt| dt.time()).map_err(|_| ()))
        .map_err(|()| SncfConnectError::Invalid(format!("invalid time {s:?}: use HH:MM")))
}

impl SncfConnect {
    pub fn new(desktop: Arc<dyn Desktop>) -> Self {
        Self::with_timing(desktop, Timing::default())
    }

    pub fn with_timing(desktop: Arc<dyn Desktop>, timing: Timing) -> Self {
        Self {
            driver: Driver::new(desktop, APP, timing.input_interval)
                .with_poll_interval(timing.poll),
            cache: TtlCache::new(timing.cache_ttl, 32),
            busy: Mutex::new(()),
            searches: Throttle::new(timing.search_interval),
            timing,
            today: None,
        }
    }

    /// Pretends today is `date` (Europe/Paris), for tests with recorded pages.
    pub fn with_today(mut self, date: NaiveDate) -> Self {
        self.today = Some(date);
        self
    }

    fn today(&self) -> NaiveDate {
        self.today.unwrap_or_else(|| time::now().date_naive())
    }

    /// The user's desktop, from `AUTOCRUISE_DESKTOP_DISPLAY` /
    /// `AUTOCRUISE_DESKTOP_DBUS` (see the connector documentation).
    pub fn from_env() -> Self {
        Self::new(Arc::new(LinuxDesktop::from_env()))
    }

    /// Train prices for one day, read from SNCF Connect in the user's browser.
    pub async fn prices(&self, params: PricesParams) -> Result<PricesResult, SncfConnectError> {
        let req = Request::parse(params, self.today())?;
        let key = req.cache_key();
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit);
        }
        let _one_at_a_time = self.busy.lock().await;
        if let Some(hit) = self.cache.get(&key) {
            return Ok(hit);
        }
        self.searches.wait().await;
        let result = self.search(&req).await?;
        self.cache.insert(key, result.clone());
        Ok(result)
    }

    /// Holds a fare with SNCF Connect's free option ("Place an option for
    /// free"). Without `confirm: true` nothing is changed: a preview is
    /// returned. Never pays: the flow stops at the option and refuses any
    /// control that pays.
    pub async fn hold_fare(
        &self,
        params: HoldFareParams,
    ) -> Result<HoldFareResult, SncfConnectError> {
        let departure = parse_time(&params.departure_time)?;
        let class = params.travel_class.unwrap_or_default();
        let prices = PricesParams {
            from: params.from.clone(),
            to: params.to.clone(),
            date: params.date.clone(),
            time_after: Some(departure.format("%H:%M").to_string()),
            passengers: params.passengers,
        };
        let req = Request::parse(prices.clone(), self.today())?;

        if params.confirm != Some(true) {
            let found = self.prices(prices).await?;
            let journey = pick_journey(&found.journeys, req.date, departure)?;
            let price = class_price(&journey, class)?;
            return Ok(HoldFareResult {
                status: "preview".into(),
                message: format!(
                    "Nothing was changed. With confirm=true, autocruise will put this {} fare in the SNCF Connect basket of the logged-in account and place SNCF Connect's free option on it, if SNCF Connect offers one for this order (not on OUIGO, TER, Eurostar or non-exchangeable fares). The price is held until the deadline SNCF Connect gives; nothing is paid. Ask the user before confirming.",
                    class_name(class)
                ),
                journey,
                travel_class: class,
                price: Some(price),
                confirmation: Vec::new(),
            });
        }

        let _one_at_a_time = self.busy.lock().await;
        self.searches.wait().await;
        let found = self.search(&req).await?;
        let journey = pick_journey(&found.journeys, req.date, departure)?;
        class_price(&journey, class)?;
        let (price, confirmation) = self.place_option(departure, class).await?;
        Ok(HoldFareResult {
            status: "held".into(),
            message: "The free option was placed on SNCF Connect (nothing paid). The user must pay it in SNCF Connect (Your tickets) before the deadline, or it lapses.".into(),
            journey,
            travel_class: class,
            price,
            confirmation,
        })
    }

    /// From the results page: fare -> basket -> free option.
    async fn place_option(
        &self,
        departure: NaiveTime,
        class: TravelClass,
    ) -> Result<(Option<f64>, Vec<String>), SncfConnectError> {
        let short = Duration::from_secs(15);
        self.click_visible("the fare list of the train", |doc| {
            page::journey_item(doc, departure)
                .and_then(page::fare_toggle)
                .cloned()
        })
        .await?;
        let price = self
            .wait_doc("the fares of the train", short, |doc| {
                page::journey_item(doc, departure)
                    .and_then(|item| page::fare_choice(item, class))
                    .map_or(Check::Pending, |(_, price)| Check::Ready(price))
            })
            .await?;
        self.click_visible("the fare to hold", |doc| {
            page::journey_item(doc, departure)
                .and_then(|item| page::fare_choice(item, class))
                .map(|(button, _)| button)
        })
        .await?;

        // Optional steps before the basket: seat preference (default kept),
        // partner offers (skipped).
        loop {
            let step = self
                .wait_doc("the basket", self.timing.page_timeout, |doc| {
                    if doc.name.starts_with(page::BASKET_TITLE) {
                        Check::Ready(Step::Basket)
                    } else if let Some(b) = doc
                        .find(|n| n.role == "dialog" && n.name == "Seating selection")
                        .and_then(|d| d.find(|n| n.role == "button" && n.name == "Continue"))
                    {
                        Check::Ready(Step::Press(b.clone()))
                    } else if doc.name.starts_with(page::SERVICES_TITLE) {
                        page::button(doc, |n| n == "Skip step")
                            .map_or(Check::Pending, |b| Check::Ready(Step::Press(b.clone())))
                    } else {
                        Check::Pending
                    }
                })
                .await?;
            match step {
                Step::Basket => break,
                Step::Press(button) => {
                    self.press(&button).await?;
                    self.driver.pause(Duration::from_millis(1500)).await;
                }
            }
        }

        let offer = self
            .wait_doc(
                "the free option in the basket",
                short,
                |doc| match page::hold_offer(doc) {
                    page::HoldOffer::Unknown => Check::Pending,
                    offer => Check::Ready(offer),
                },
            )
            .await;
        let control = match offer {
            Ok(page::HoldOffer::Offered(control)) => control,
            Ok(page::HoldOffer::Refused(reason)) => {
                self.remove_from_basket().await?;
                return Err(SncfConnectError::HoldNotOffered(reason));
            }
            Ok(page::HoldOffer::Unknown) | Err(_) => {
                self.remove_from_basket().await?;
                return Err(SncfConnectError::HoldNotOffered(
                    "no free option was shown in the basket".into(),
                ));
            }
        };

        // Insurance is a mandatory choice on the basket: never buy it for the user.
        let app = self.snapshot().await?;
        if let Some(doc) = page::document(&app)
            && doc
                .find(|n| n.role == "radio button" && n.name.contains("insure this trip"))
                .is_some()
            && doc
                .find(|n| {
                    n.role == "radio button"
                        && n.name.contains("insure this trip")
                        && n.has_state("checked")
                })
                .is_none()
        {
            self.click_visible("No, do not insure this trip", |doc| {
                doc.find(|n| n.role == "radio button" && n.name == "No, do not insure this trip")
                    .cloned()
            })
            .await?;
        }
        self.click_visible("Place an option for free", |doc| {
            match page::hold_offer(doc) {
                page::HoldOffer::Offered(n) if n.name == control.name => Some(n),
                _ => None,
            }
        })
        .await?;
        self.click_visible("Confirm your option", |doc| {
            page::button(doc, |n| n.starts_with("Confirm your option")).cloned()
        })
        .await?;
        let confirmation = self
            .wait_doc("the option confirmation", self.timing.page_timeout, |doc| {
                if page::button(doc, |n| n.starts_with("Confirm your option")).is_some() {
                    return Check::Pending;
                }
                let lines: Vec<String> = doc
                    .texts()
                    .into_iter()
                    .map(page::normalize)
                    .filter(|t| t.to_lowercase().contains("option"))
                    .take(6)
                    .collect();
                if lines.is_empty() {
                    Check::Pending
                } else {
                    Check::Ready(lines)
                }
            })
            .await?;
        Ok((price, confirmation))
    }

    /// Removes the trip just added, so a refused hold leaves no trace.
    async fn remove_from_basket(&self) -> Result<(), SncfConnectError> {
        self.click_visible("Remove from cart", |doc| {
            page::button(doc, |n| n.starts_with("Remove from cart")).cloned()
        })
        .await?;
        self.click_visible("Yes, remove it", |doc| {
            page::button(doc, |n| n == "Yes, remove it").cloned()
        })
        .await?;
        self.wait_doc("the trip removal", Duration::from_secs(15), |doc| {
            if page::button(doc, |n| n.starts_with("Remove from cart")).is_none() {
                Check::Ready(())
            } else {
                Check::Pending
            }
        })
        .await
    }

    async fn search(&self, req: &Request) -> Result<PricesResult, SncfConnectError> {
        self.open_search_form().await?;
        self.set_place(page::DEPARTURE, &req.from).await?;
        self.set_place(page::ARRIVAL, &req.to).await?;
        let hour = self.set_date(req.date, req.time_after).await?;
        self.clear_direct_filter().await?;
        if req.passengers > 1 {
            self.add_passengers(req.passengers - 1).await?;
        }
        let doc = self.submit().await?;
        let results =
            page::parse_results(&doc, req.date).map_err(SncfConnectError::UnexpectedPage)?;

        let start_time = req
            .time_after
            .unwrap_or_else(|| NaiveTime::from_hms_opt(hour, 0, 0).expect("valid hour"));
        let start = time::localize(req.date.and_time(start_time))?;
        let journeys = results
            .journeys
            .into_iter()
            .filter(|j| time::parse_datetime(&j.departure).is_ok_and(|d| d >= start))
            .collect();
        Ok(PricesResult {
            origin: results.origin,
            destination: results.destination,
            date: req.date.format("%Y-%m-%d").to_string(),
            searched_from: time::format(&start),
            passengers: req.passengers,
            currency: "EUR".into(),
            journeys,
            cheapest_per_day: results.days,
            notices: results.notices,
            more_later: results.more_later,
        })
    }

    /// Current tree, failing on a challenge.
    async fn snapshot(&self) -> Result<Node, SncfConnectError> {
        let app = self.driver.snapshot().await?;
        if let Some(marker) = page::challenge(&app) {
            return Err(SncfConnectError::Challenge(marker));
        }
        Ok(app)
    }

    /// Waits until `ready` returns a value from the current page. A
    /// challenge anywhere stops immediately.
    async fn wait_doc<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut ready: impl FnMut(&Node) -> Check<T, SncfConnectError>,
    ) -> Result<T, SncfConnectError> {
        self.driver
            .wait_until(what, timeout, |app| {
                if let Some(marker) = page::challenge(app) {
                    return Check::Fail(SncfConnectError::Challenge(marker));
                }
                match page::document(app) {
                    Some(doc) => ready(doc),
                    None => Check::Pending,
                }
            })
            .await
    }

    /// Like `wait_doc`, for a node about to be clicked (see
    /// [`Driver::wait_until_stable`]).
    async fn stable_node(
        &self,
        what: &str,
        timeout: Duration,
        mut find: impl FnMut(&Node) -> Check<Node, SncfConnectError>,
    ) -> Result<Node, SncfConnectError> {
        self.driver
            .wait_until_stable(what, timeout, |app| {
                if let Some(marker) = page::challenge(app) {
                    return Check::Fail(SncfConnectError::Challenge(marker));
                }
                match page::document(app) {
                    Some(doc) => find(doc),
                    None => Check::Pending,
                }
            })
            .await
    }

    /// Clicks a node, refusing anything that could pay.
    async fn press(&self, node: &Node) -> Result<(), SncfConnectError> {
        if page::is_payment(&node.name) {
            return Err(SncfConnectError::PaymentRefused(node.name.clone()));
        }
        Ok(self.driver.click(node).await?)
    }

    /// Clicks the node `find` returns, scrolling it into view first.
    async fn click_visible(
        &self,
        what: &str,
        find: impl Fn(&Node) -> Option<Node>,
    ) -> Result<(), SncfConnectError> {
        for _ in 0..8 {
            let node = self
                .stable_node(what, Duration::from_secs(10), |doc| {
                    find(doc).map_or(Check::Pending, Check::Ready)
                })
                .await?;
            let app = self.snapshot().await?;
            let view = page::document(&app)
                .and_then(|d| d.bounds)
                .unwrap_or(Rect::new(0, 0, 1920, 1080));
            let (x, y) = node
                .center()
                .ok_or_else(|| DesktopError::NotClickable(what.to_owned()))?;
            if view.contains(x, y) {
                return self.press(&node).await;
            }
            let (_, vy) = view.center();
            let sx = x.clamp(view.x + 1, view.x + view.width - 2);
            let notches = if y > vy { 3 } else { -3 };
            self.driver.scroll(sx, vy, notches).await?;
            self.driver.pause(Duration::from_millis(500)).await;
        }
        Err(SncfConnectError::UnexpectedPage(format!(
            "could not bring {what} into view"
        )))
    }

    async fn open_search_form(&self) -> Result<(), SncfConnectError> {
        let app = self.snapshot().await?;
        let bar = page::address_bar(&app).ok_or_else(|| {
            SncfConnectError::UnexpectedPage(
                "Chrome's address bar was not found; is the browser window open?".into(),
            )
        })?;
        self.press(bar).await?;
        self.driver.key("ctrl+a").await?;
        self.driver.type_text(page::SEARCH_URL).await?;
        self.driver.key("Return").await?;
        self.driver.pause(Duration::from_millis(1500)).await;

        // The header shows "Sign in" for a moment before the session loads.
        let mut logged_out_reads = 0;
        self.wait_doc(
            "the SNCF Connect search form",
            self.timing.page_timeout,
            |doc| {
                if !page::is_search_form(doc) {
                    return Check::Pending;
                }
                match page::session(doc) {
                    Session::LoggedIn => Check::Ready(()),
                    Session::LoggedOut if logged_out_reads >= 5 => {
                        Check::Fail(SncfConnectError::LoggedOut)
                    }
                    Session::LoggedOut => {
                        logged_out_reads += 1;
                        Check::Pending
                    }
                    Session::Unknown => Check::Pending,
                }
            },
        )
        .await
    }

    async fn set_place(&self, label: &str, query: &str) -> Result<(), SncfConnectError> {
        let app = self.snapshot().await?;
        let field = page::document(&app)
            .and_then(|d| page::combo(d, label))
            .ok_or_else(|| SncfConnectError::UnexpectedPage(format!("no {label} field")))?;
        self.press(field).await?;
        self.driver.key("ctrl+a").await?;
        self.driver.type_text(query).await?;

        // The list refreshes as results arrive: act only on a stable list.
        let what = format!("suggestions for {query:?}");
        let chosen = self
            .stable_node(&what, Duration::from_secs(15), |doc| {
                let items = page::suggestions(doc, label);
                if !page::suggestions_match(&items, query) {
                    return Check::Pending;
                }
                match page::pick_suggestion(&items, query) {
                    Some(s) => Check::Ready(s.node.clone()),
                    None => Check::Fail(SncfConnectError::UnknownPlace(query.to_owned())),
                }
            })
            .await
            .map_err(|e| match e {
                SncfConnectError::Desktop(autocruise_desktop::DesktopError::Timeout(_)) => {
                    SncfConnectError::UnknownPlace(query.to_owned())
                }
                e => e,
            })?;
        let chosen_name = page::suggestion_name(&chosen);
        self.press(&chosen).await?;

        let fold = |s: &str| s.to_lowercase();
        self.wait_doc(
            &format!("{label} set to {chosen_name}"),
            Duration::from_secs(8),
            |doc| {
                let value = page::combo(doc, label)
                    .and_then(|c| c.value.clone())
                    .unwrap_or_default();
                let listed = !page::suggestions(doc, label).is_empty();
                if !listed && !value.is_empty() && fold(&value).contains(&fold(&chosen_name)) {
                    Check::Ready(())
                } else {
                    Check::Pending
                }
            },
        )
        .await
    }

    /// Sets the outbound date and the time slot; returns the slot hour.
    async fn set_date(
        &self,
        date: NaiveDate,
        time_after: Option<NaiveTime>,
    ) -> Result<u32, SncfConnectError> {
        let app = self.snapshot().await?;
        let open = page::document(&app)
            .and_then(|d| page::button(d, |n| n.starts_with("Outbound:")))
            .ok_or_else(|| SncfConnectError::UnexpectedPage("no outbound date button".into()))?;
        self.press(open).await?;

        let short = Duration::from_secs(10);
        let entry = self
            .wait_doc("the date dialog", short, |doc| {
                doc.find(|n| n.role == "dialog" && n.name == page::DATE_DIALOG)
                    .and_then(|d| d.find(|n| n.role == "entry" && n.name == page::DATE_ENTRY))
                    .map_or(Check::Pending, |e| Check::Ready(e.clone()))
            })
            .await?;
        let typed = date.format("%d/%m/%Y").to_string();
        self.press(&entry).await?;
        self.driver.key("ctrl+a").await?;
        self.driver.type_text(&typed).await?;
        self.driver.pause(Duration::from_millis(500)).await;

        let app = self.snapshot().await?;
        let combo = page::document(&app)
            .and_then(|d| d.find(|n| n.role == "combo box" && n.name.starts_with(page::TIME_COMBO)))
            .ok_or_else(|| SncfConnectError::UnexpectedPage("no time selector".into()))?;
        self.press(combo).await?;
        let wanted = time_after.map_or(0, |t| t.hour());
        // The list opens with an animation: wait until it stops moving.
        let option = self
            .stable_node("the time slots", short, |doc| {
                let options = page::hour_options(doc);
                let labels: Vec<String> =
                    options.iter().map(|o| page::normalize(&o.name)).collect();
                match page::pick_hour(&labels, wanted) {
                    Some(i) => Check::Ready(options[i].clone()),
                    None => Check::Pending,
                }
            })
            .await?;
        let label = page::normalize(&option.name);
        self.press(&option).await?;
        let confirm = self
            .wait_doc("the chosen time slot", short, |doc| {
                let combo_ok = doc
                    .find(|n| n.role == "combo box" && n.name.starts_with(page::TIME_COMBO))
                    .is_some_and(|c| page::normalize(&c.name).ends_with(&label));
                let entry_ok = doc
                    .find(|n| n.role == "entry" && n.name == page::DATE_ENTRY)
                    .is_some_and(|e| e.value.as_deref() == Some(typed.as_str()));
                let confirm = page::button(doc, |n| n == "Confirm");
                match (combo_ok && entry_ok, confirm) {
                    (true, Some(b)) => Check::Ready(b.clone()),
                    _ => Check::Pending,
                }
            })
            .await
            .map_err(|_| {
                SncfConnectError::Invalid(format!(
                    "SNCF Connect did not accept the date {date} (too far ahead or not on sale?)"
                ))
            })?;
        self.press(&confirm).await?;
        self.wait_doc("the date dialog to close", short, |doc| {
            if doc
                .find(|n| n.role == "dialog" && n.name == page::DATE_DIALOG)
                .is_none()
            {
                Check::Ready(())
            } else {
                Check::Pending
            }
        })
        .await?;
        Ok(page::parse_hour_label(&label).unwrap_or(wanted))
    }

    /// A previous search may have left "Direct travel" on: results must be
    /// complete.
    async fn clear_direct_filter(&self) -> Result<(), SncfConnectError> {
        let app = self.snapshot().await?;
        if let Some(check) = page::document(&app).and_then(|d| {
            d.find(|n| {
                n.role == "check box" && n.name == "Select Direct travel" && n.has_state("checked")
            })
        }) {
            self.press(check).await?;
        }
        Ok(())
    }

    /// Adds `extra` anonymous travellers (adults 30-59, the panel's default).
    async fn add_passengers(&self, extra: u8) -> Result<(), SncfConnectError> {
        let short = Duration::from_secs(10);
        self.click_visible("Add a passenger", |doc| {
            page::button(doc, |n| n == "Add a passenger").cloned()
        })
        .await?;
        for added in 1..=usize::from(extra) {
            self.click_visible("Add a traveler", |doc| {
                page::button(doc, |n| n == "Add a traveler" || n == "Add a traveller").cloned()
            })
            .await?;
            self.wait_doc("a new traveller", short, |doc| {
                let count = doc
                    .find_all(|n| n.role == "panel" && n.name.starts_with("New traveller "))
                    .len();
                if count >= added {
                    Check::Ready(())
                } else {
                    Check::Pending
                }
            })
            .await?;
        }
        let confirm = |n: &str| n == "Add this passenger" || n == "Add these passengers";
        self.click_visible("Add this passenger", |doc| {
            page::button(doc, confirm).cloned()
        })
        .await?;
        self.wait_doc("the search form", short, |doc| {
            if page::button(doc, confirm).is_none() {
                Check::Ready(())
            } else {
                Check::Pending
            }
        })
        .await
    }

    async fn submit(&self) -> Result<Node, SncfConnectError> {
        let app = self.snapshot().await?;
        let see = page::document(&app)
            .and_then(|d| page::button(d, |n| n == "See prices"))
            .ok_or_else(|| SncfConnectError::UnexpectedPage("no See prices button".into()))?;
        self.press(see).await?;
        self.driver.pause(Duration::from_millis(1500)).await;
        self.wait_doc("the price results", self.timing.page_timeout, |doc| {
            if page::results_ready(doc) {
                Check::Ready(doc.clone())
            } else {
                Check::Pending
            }
        })
        .await
    }
}

enum Step {
    Basket,
    Press(Node),
}

fn class_name(class: TravelClass) -> &'static str {
    match class {
        TravelClass::Second => "second-class",
        TravelClass::First => "first-class",
    }
}

fn pick_journey(
    journeys: &[Journey],
    date: NaiveDate,
    departure: NaiveTime,
) -> Result<Journey, SncfConnectError> {
    let wanted = time::localize(date.and_time(departure)).map(|d| time::format(&d))?;
    journeys
        .iter()
        .find(|j| j.departure == wanted)
        .cloned()
        .ok_or_else(|| {
            SncfConnectError::JourneyNotFound(format!(
                "no train leaving at {} on {date} in the SNCF Connect results; check sncf_connect_prices",
                departure.format("%H:%M")
            ))
        })
}

fn class_price(journey: &Journey, class: TravelClass) -> Result<f64, SncfConnectError> {
    let price = match class {
        TravelClass::Second => journey.price_second_class,
        TravelClass::First => journey.price_first_class,
    };
    price.filter(|_| !journey.fully_booked).ok_or_else(|| {
        SncfConnectError::JourneyNotFound(format!(
            "no {} fare is on sale for this train",
            class_name(class)
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(date: &str) -> PricesParams {
        PricesParams {
            from: "Paris".into(),
            to: "Lyon".into(),
            date: date.into(),
            time_after: Some("08:30".into()),
            passengers: None,
        }
    }

    #[test]
    fn request_validation() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let parse = |p| Request::parse(p, today);
        let req = parse(params("2026-10-21")).unwrap();
        assert_eq!(req.time_after, NaiveTime::from_hms_opt(8, 30, 0));
        assert_eq!(req.passengers, 1);
        assert!(matches!(
            parse(params("2026-10-19")),
            Err(SncfConnectError::Invalid(_))
        ));
        assert!(
            parse(PricesParams {
                passengers: Some(9),
                ..params("2026-10-21")
            })
            .is_err()
        );
        assert!(
            parse(PricesParams {
                time_after: Some("8h".into()),
                ..params("2026-10-21")
            })
            .is_err()
        );
        assert!(
            parse(PricesParams {
                from: " ".into(),
                ..params("2026-10-21")
            })
            .is_err()
        );
    }
}
