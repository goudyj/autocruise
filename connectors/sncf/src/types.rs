//! Tool inputs and outputs. These shapes are what AI assistants see,
//! so field docs double as prompts: keep them short and precise.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------- shared ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    /// A train station (stop area).
    Station,
    /// A city or administrative area.
    City,
    /// A street address.
    Address,
    /// A point of interest.
    Poi,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Place {
    /// Identifier to pass to other sncf_* tools.
    pub id: String,
    pub name: String,
    pub kind: PlaceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    /// Distance from the searched point, in meters (nearby search only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_m: Option<u32>,
}

/// A place resolved from the user's input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedPlace {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DisruptionBrief {
    /// Navitia effect, e.g. NO_SERVICE (cancelled), SIGNIFICANT_DELAYS, REDUCED_SERVICE, DETOUR, MODIFIED_SERVICE.
    pub effect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ---------- sncf_search_places ----------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchPlacesParams {
    /// Free text, e.g. "Lyon Part-Dieu", "Bordeaux", "gare de l'Est".
    pub query: String,
    /// Kinds of places to return. Default: stations and cities.
    #[serde(default)]
    pub kinds: Option<Vec<PlaceKind>>,
    /// Maximum results (1-20, default 5).
    #[serde(default)]
    pub count: Option<u8>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchPlacesResult {
    pub places: Vec<Place>,
}

// ---------- sncf_nearby_stations ----------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct NearbyStationsParams {
    pub latitude: f64,
    pub longitude: f64,
    /// Search radius in meters (50-5000, default 1000).
    #[serde(default)]
    pub radius_m: Option<u32>,
    /// Maximum results (1-20, default 5).
    #[serde(default)]
    pub count: Option<u8>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NearbyStationsResult {
    pub stations: Vec<Place>,
}

// ---------- sncf_plan_journey ----------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DatetimeRepresents {
    /// Leave at or after `datetime`.
    #[default]
    Departure,
    /// Arrive at or before `datetime`.
    Arrival,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PlanJourneyParams {
    /// Origin: a place id from sncf_search_places, "latitude,longitude", or a station/city name (best match is used).
    pub from: String,
    /// Destination, same formats as `from`.
    pub to: String,
    /// ISO 8601 date-time. Without offset it is Europe/Paris local time. Default: now.
    #[serde(default)]
    pub datetime: Option<String>,
    /// Whether `datetime` is the departure (default) or the arrival time.
    #[serde(default)]
    pub datetime_represents: Option<DatetimeRepresents>,
    /// Number of journeys (1-10, default 3).
    #[serde(default)]
    pub count: Option<u8>,
    /// Maximum number of transfers.
    #[serde(default)]
    pub max_transfers: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LegKind {
    /// On board a train, coach or other public transport.
    Ride,
    /// Walking or other personal mode.
    Walk,
    /// Changing platform/station within a transfer.
    Transfer,
    /// Abstract link between a city/address and the first/last station.
    Access,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Leg {
    pub kind: LegKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Expected (real-time when available) departure, Europe/Paris with offset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure: Option<String>,
    /// Expected (real-time when available) arrival, Europe/Paris with offset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrival: Option<String>,
    pub duration_minutes: i64,
    /// Commercial mode, e.g. "TGV INOUI", "OUIGO", "TER", "Intercités".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub train_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// Departure delay in minutes versus the timetable (0 if on time or unknown).
    #[serde(skip_serializing_if = "is_zero")]
    #[schemars(default)]
    pub delay_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Journey {
    pub departure: String,
    pub arrival: String,
    pub duration_minutes: i64,
    pub transfers: u32,
    /// Worst disruption effect on this journey, if any (e.g. SIGNIFICANT_DELAYS, NO_SERVICE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub co2_grams: Option<f64>,
    pub legs: Vec<Leg>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schemars(default)]
    pub disruptions: Vec<DisruptionBrief>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PlanJourneyResult {
    pub from: ResolvedPlace,
    pub to: ResolvedPlace,
    /// Requested time, Europe/Paris with offset.
    pub datetime: String,
    pub datetime_represents: DatetimeRepresents,
    pub journeys: Vec<Journey>,
}

// ---------- sncf_departures / sncf_arrivals ----------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BoardParams {
    /// Station: a stop_area id from sncf_search_places, or a station name (best match is used).
    pub station: String,
    /// Start of the window, ISO 8601. Without offset it is Europe/Paris local time. Default: now.
    #[serde(default)]
    pub datetime: Option<String>,
    /// Maximum results (1-50, default 10).
    #[serde(default)]
    pub count: Option<u8>,
    /// Only return trains within this many minutes after `datetime`.
    #[serde(default)]
    pub duration_minutes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct BoardEntry {
    /// Expected time (real-time when available), Europe/Paris with offset.
    pub time: String,
    /// Timetable time, Europe/Paris with offset.
    pub scheduled_time: String,
    /// Delay in minutes (0 if on time).
    pub delay_minutes: i64,
    /// True when the time comes from real-time data.
    pub realtime: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub train_number: Option<String>,
    /// Final destination of the train.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schemars(default)]
    pub disruptions: Vec<DisruptionBrief>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BoardResult {
    pub station: ResolvedPlace,
    /// Start of the window, Europe/Paris with offset.
    pub since: String,
    pub trains: Vec<BoardEntry>,
}

// ---------- sncf_train ----------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TrainParams {
    /// Train number as shown on tickets and boards, e.g. "6623".
    pub train_number: String,
    /// Travel date YYYY-MM-DD (Europe/Paris). Default: today.
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct TrainStop {
    pub station: String,
    /// Scheduled arrival, Europe/Paris with offset (absent at the origin).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrival: Option<String>,
    /// Scheduled departure, Europe/Paris with offset (absent at the terminus).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure: Option<String>,
    /// True if the train does not stop here anymore (disruption).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[schemars(default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Train {
    pub train_number: String,
    pub stops: Vec<TrainStop>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[schemars(default)]
    pub disruptions: Vec<DisruptionBrief>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TrainResult {
    pub date: String,
    /// Usually one; empty if no train runs with this number on that date.
    pub trains: Vec<Train>,
}

// ---------- sncf_disruptions ----------

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DisruptionsParams {
    /// Optional station (stop_area id or name). Without it, network-wide disruptions are listed.
    #[serde(default)]
    pub station: Option<String>,
    /// Day to inspect, YYYY-MM-DD (Europe/Paris). Default: today. Ignored when `station` is set.
    #[serde(default)]
    pub date: Option<String>,
    /// Maximum results (1-25, default 10).
    #[serde(default)]
    pub count: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Period {
    pub begin: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Disruption {
    pub id: String,
    /// "active" or "future" (past disruptions are filtered out).
    pub status: String,
    /// Navitia effect, e.g. NO_SERVICE (cancelled), SIGNIFICANT_DELAYS, REDUCED_SERVICE.
    pub effect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    pub periods: Vec<Period>,
    /// Impacted trains ("train 6623") or other objects (lines, stations).
    pub impacted: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DisruptionsResult {
    /// What was searched: a station name or "network".
    pub scope: String,
    pub disruptions: Vec<Disruption>,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}
