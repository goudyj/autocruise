//! SNCF connector for autocruise.
//!
//! Uses the official SNCF API (Navitia): <https://numerique.sncf.com/startup/api/>.
//! Read-only: stations, journeys, departures/arrivals with real-time delays,
//! train details and disruptions. **No prices and no booking**: the API does
//! not provide them.
//!
//! Times: the API speaks Europe/Paris local time without offset. This crate
//! sends request times as UTC (`...Z`) and returns RFC 3339 times in
//! Europe/Paris with an explicit offset.
//!
//! Layout:
//! - [`client`]: HTTP, auth, error mapping, throttling, cache.
//! - `navitia`: raw API response types.
//! - [`types`]: tool inputs and outputs.
//! - `convert`: pure conversions raw → outputs.
//! - `resolve`: user input (ids, coordinates, names) → API place ids.
//! - `service`: [`Sncf`], one async method per tool.

pub mod client;
mod convert;
mod error;
mod navitia;
mod resolve;
mod service;
pub mod types;

pub use client::SncfClient;
pub use error::SncfError;
pub use service::Sncf;
