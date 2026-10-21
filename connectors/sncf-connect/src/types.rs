//! Tool inputs and outputs. Field docs are read by the model.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct PricesParams {
    /// Departure city or station, as typed on SNCF Connect (e.g. "Paris",
    /// "Lyon Part-Dieu").
    pub from: String,
    /// Arrival city or station.
    pub to: String,
    /// Travel date, YYYY-MM-DD (Europe/Paris).
    pub date: String,
    /// Only trains leaving at or after this local time, HH:MM (Europe/Paris).
    /// Default: from midnight.
    #[serde(default)]
    pub time_after: Option<String>,
    /// Only trains leaving before this local time, HH:MM (Europe/Paris).
    /// Default: until the end of the day. Setting it makes the search
    /// faster (fewer "next journeys" pages to load).
    #[serde(default)]
    pub time_before: Option<String>,
    /// Number of travellers, 1 to 8 (default 1). The first is the logged-in
    /// user with their discount cards; extra travellers are added as adults
    /// (30-59) without discount card.
    #[serde(default)]
    #[schemars(range(min = 1, max = 8))]
    pub passengers: Option<u8>,
    /// Only journeys without any connection (default false).
    #[serde(default)]
    pub direct_only: Option<bool>,
    /// Only these train types, e.g. ["tgv_inoui", "ouigo"] for high-speed
    /// trains only. Journeys with connections have no single type and are
    /// left out when this is set. Default: all.
    #[serde(default)]
    pub train_types: Option<Vec<TrainType>>,
}

/// Train product, as labelled by SNCF Connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrainType {
    /// TGV INOUI (high speed).
    TgvInoui,
    /// OUIGO Grande Vitesse (low-cost high speed).
    Ouigo,
    /// OUIGO Train Classique (low-cost, conventional lines).
    OuigoClassique,
    /// TER (regional).
    Ter,
    /// Intercités, including night trains.
    Intercites,
    /// TGV Lyria (France - Switzerland).
    Lyria,
    /// Eurostar.
    Eurostar,
    /// Any other product (foreign operators, buses...).
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PricesResult {
    /// Departure and arrival as shown by SNCF Connect.
    pub origin: String,
    pub destination: String,
    /// Travel date, YYYY-MM-DD.
    pub date: String,
    /// Results start at this time (RFC 3339, Europe/Paris).
    pub searched_from: String,
    /// Results end before this time (RFC 3339, Europe/Paris), when
    /// time_before was given.
    pub searched_until: Option<String>,
    pub passengers: u8,
    /// Always "EUR".
    pub currency: String,
    /// Journeys matching the filters, sorted by departure time.
    pub journeys: Vec<Journey>,
    /// The cheapest journey of `journeys` (any class, not fully booked);
    /// null when none has a fare.
    pub cheapest: Option<Cheapest>,
    /// Journeys read but left out by direct_only / train_types.
    pub filtered_out: u32,
    /// Cheapest fare of each day around the date, as shown in the date strip.
    pub cheapest_per_day: Vec<DayPrice>,
    /// Route notices shown by SNCF Connect (e.g. discount card not applicable).
    pub notices: Vec<String>,
    /// More trains exist later the same day than those returned (after
    /// time_before, or beyond the pages autocruise loads): call again with a
    /// later time_after.
    pub more_later: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Cheapest {
    /// Index in `journeys`.
    pub index: usize,
    /// Departure of that journey (RFC 3339, Europe/Paris).
    pub departure: String,
    pub travel_class: TravelClass,
    /// EUR.
    pub price: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Journey {
    /// Departure time, RFC 3339 Europe/Paris with offset.
    pub departure: String,
    /// Arrival time, RFC 3339 Europe/Paris with offset.
    pub arrival: String,
    pub duration_minutes: Option<u32>,
    pub origin_station: String,
    pub destination_station: String,
    /// Label shown by SNCF Connect ("TGV INOUI", "OUIGO Grande Vitesse",
    /// "TER"...); null for journeys with connections.
    pub train_type: Option<String>,
    /// The same, as a category usable in train_types; null for journeys
    /// with connections.
    pub train_category: Option<TrainType>,
    /// Train number(s) (e.g. ["6623"]). Only read when several results
    /// leave at the same time (coupled trains sold separately, often with
    /// different fares); empty otherwise.
    pub train_numbers: Vec<String>,
    pub direct: bool,
    pub connections: u32,
    /// Lowest second-class fare per traveller set, EUR; null when not sold.
    pub price_second_class: Option<f64>,
    /// Lowest first-class fare, EUR; null when not sold.
    pub price_first_class: Option<f64>,
    /// "N seats at this price" when SNCF Connect shows it.
    pub seats_left_second_class: Option<u32>,
    pub seats_left_first_class: Option<u32>,
    /// SNCF Connect shows the train as fully booked.
    pub fully_booked: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TravelClass {
    #[default]
    Second,
    First,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct HoldFareParams {
    /// Departure city or station, as for sncf_connect_prices.
    pub from: String,
    /// Arrival city or station.
    pub to: String,
    /// Travel date, YYYY-MM-DD (Europe/Paris).
    pub date: String,
    /// Departure time of the train to hold, HH:MM (Europe/Paris), as
    /// returned by sncf_connect_prices.
    pub departure_time: String,
    /// "second" (default) or "first": the standard fare of that class is held.
    #[serde(default)]
    pub travel_class: Option<TravelClass>,
    /// Train number, as returned by sncf_connect_prices in train_numbers.
    /// Required only when several trains leave at departure_time.
    #[serde(default)]
    pub train_number: Option<String>,
    /// Number of travellers, 1 to 8 (default 1), as for sncf_connect_prices.
    #[serde(default)]
    #[schemars(range(min = 1, max = 8))]
    pub passengers: Option<u8>,
    /// Must be true to actually place the option. Without it, nothing is
    /// changed and a preview is returned: show it to the user and ask them.
    #[serde(default)]
    pub confirm: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct HoldFareResult {
    /// "preview" (nothing done) or "held" (option placed, not paid).
    pub status: String,
    pub journey: Journey,
    pub travel_class: TravelClass,
    /// Price that will be (or was) held, EUR.
    pub price: Option<f64>,
    /// What SNCF Connect said about the option (deadline...), when held.
    pub confirmation: Vec<String>,
    /// What happened or will happen, for the user.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct DayPrice {
    /// YYYY-MM-DD.
    pub date: String,
    /// Cheapest fare that day, EUR; null when none is shown.
    pub cheapest_price: Option<f64>,
}
