//! Shared building blocks for autocruise connectors.
//!
//! - [`time`]: everything is expressed in the France time zone (Europe/Paris),
//!   with explicit UTC offsets in outputs.
//! - [`http`]: the HTTP transport abstraction (real and mock), so connectors
//!   can be tested without network.
//! - [`error`]: error categories shared by all connectors.
//! - [`throttle`]: keep requests at a human pace.
//! - [`cache`]: short-lived in-memory cache for reference data.

pub mod cache;
pub mod error;
pub mod http;
pub mod throttle;
pub mod time;
