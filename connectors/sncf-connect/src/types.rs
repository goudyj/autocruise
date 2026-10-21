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
    /// SNCF Connect searches by 2-hour slots; one call returns the first page
    /// of results (about 5 to 10 trains). Call again with a later time for
    /// later trains.
    #[serde(default)]
    pub time_after: Option<String>,
    /// Number of travellers, 1 to 8 (default 1). The first is the logged-in
    /// user with their discount cards; extra travellers are added as adults
    /// (30-59) without discount card.
    #[serde(default)]
    #[schemars(range(min = 1, max = 8))]
    pub passengers: Option<u8>,
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
    pub passengers: u8,
    /// Always "EUR".
    pub currency: String,
    /// Trains, in departure order.
    pub journeys: Vec<Journey>,
    /// Cheapest fare of each day around the date, as shown in the date strip.
    pub cheapest_per_day: Vec<DayPrice>,
    /// Route notices shown by SNCF Connect (e.g. discount card not applicable).
    pub notices: Vec<String>,
    /// More trains exist later in the day (call again with a later time_after).
    pub more_later: bool,
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
    /// "TGV INOUI", "OUIGO", "TER"... null for journeys with connections.
    pub train_type: Option<String>,
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
