//! Connector registration. Adding a connector = a new module here exposing
//! `register`, plus one `Connector` variant.

pub mod sncf;
pub mod sncf_connect;

use crate::server::Registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Connector {
    /// SNCF trains (official API, needs SNCF_API_TOKEN).
    Sncf,
    /// SNCF Connect prices and fare hold (unofficial): drives the user's own
    /// logged-in Chrome on their desktop (accessibility + real input).
    SncfConnect,
}

impl Connector {
    /// Registers the connector, configured from the environment.
    pub fn register(self, registry: &mut Registry) {
        match self {
            Self::Sncf => sncf::register(registry, autocruise_sncf::Sncf::from_env()),
            Self::SncfConnect => {
                sncf_connect::register(registry, autocruise_sncf_connect::SncfConnect::from_env())
            }
        }
    }
}
