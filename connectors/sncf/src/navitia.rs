//! Subset of the Navitia response format used by the connector.
//! Every field is optional/defaulted: the API omits fields freely.

use serde::Deserialize;

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Coord {
    pub lat: String,
    pub lon: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct AdminRegion {
    pub id: String,
    pub name: String,
    pub label: String,
    pub level: Option<u32>,
    pub zip_code: Option<String>,
    pub coord: Coord,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct StopArea {
    pub id: String,
    pub name: String,
    pub label: String,
    pub coord: Coord,
    pub administrative_regions: Vec<AdminRegion>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Address {
    pub id: String,
    pub name: String,
    pub label: String,
    pub coord: Coord,
    pub administrative_regions: Vec<AdminRegion>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub embedded_type: String,
    pub stop_area: Option<StopArea>,
    pub administrative_region: Option<AdminRegion>,
    pub address: Option<Address>,
    pub poi: Option<Address>,
    pub distance: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlacesResponse {
    pub places: Vec<Place>,
    pub places_nearby: Vec<Place>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Link {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayInformations {
    pub commercial_mode: String,
    pub network: String,
    pub direction: String,
    pub label: String,
    pub headsign: String,
    pub trip_short_name: String,
    pub name: String,
    pub links: Vec<Link>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct StopDateTime {
    pub departure_date_time: Option<String>,
    pub base_departure_date_time: Option<String>,
    pub arrival_date_time: Option<String>,
    pub base_arrival_date_time: Option<String>,
    pub data_freshness: Option<String>,
    pub links: Vec<Link>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Passage {
    pub display_informations: DisplayInformations,
    pub stop_date_time: StopDateTime,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BoardResponse {
    pub departures: Vec<Passage>,
    pub arrivals: Vec<Passage>,
    pub disruptions: Vec<Disruption>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Section {
    #[serde(rename = "type")]
    pub kind: String,
    pub mode: Option<String>,
    pub duration: i64,
    pub departure_date_time: Option<String>,
    pub arrival_date_time: Option<String>,
    pub base_departure_date_time: Option<String>,
    pub base_arrival_date_time: Option<String>,
    pub from: Option<Place>,
    pub to: Option<Place>,
    pub display_informations: Option<DisplayInformations>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Co2 {
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Journey {
    pub departure_date_time: String,
    pub arrival_date_time: String,
    pub duration: i64,
    pub nb_transfers: u32,
    pub status: String,
    pub co2_emission: Option<Co2>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct JourneysResponse {
    pub journeys: Vec<Journey>,
    pub disruptions: Vec<Disruption>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct StopPoint {
    pub id: String,
    pub name: String,
    pub label: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct StopTime {
    pub arrival_time: Option<String>,
    pub departure_time: Option<String>,
    pub stop_point: StopPoint,
    pub pickup_allowed: bool,
    pub drop_off_allowed: bool,
    pub skipped_stop: bool,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct VehicleJourney {
    pub id: String,
    pub name: String,
    pub headsign: String,
    pub stop_times: Vec<StopTime>,
    pub disruptions: Vec<Link>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct VehicleJourneysResponse {
    pub vehicle_journeys: Vec<VehicleJourney>,
    pub disruptions: Vec<Disruption>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Severity {
    pub name: String,
    pub effect: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Message {
    pub text: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Period {
    pub begin: String,
    pub end: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Named {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct PtObject {
    pub id: String,
    pub name: String,
    pub embedded_type: String,
    pub trip: Option<Named>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct ImpactedObject {
    pub pt_object: Option<PtObject>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct Disruption {
    pub id: String,
    pub status: String,
    pub cause: String,
    pub updated_at: String,
    pub severity: Severity,
    pub messages: Vec<Message>,
    pub application_periods: Vec<Period>,
    pub impacted_objects: Vec<ImpactedObject>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DisruptionsResponse {
    pub disruptions: Vec<Disruption>,
}
