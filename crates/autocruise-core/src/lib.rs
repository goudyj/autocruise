//! Shared building blocks for autocruise connectors.
//!
//! - [`time`]: everything is expressed in the France time zone (Europe/Paris),
//!   with explicit UTC offsets in outputs.
//! - [`throttle`]: keep requests at a human pace.
//! - [`cache`]: short-lived in-memory cache for reference data.

pub mod cache;
pub mod throttle;
pub mod time;
