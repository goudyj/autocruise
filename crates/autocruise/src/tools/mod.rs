//! Connector registration. Adding a connector = a new module here exposing
//! `register`, plus one `Connector` variant.

pub mod sncf;

use crate::server::Registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Connector {
    /// SNCF trains (official API, needs SNCF_API_TOKEN).
    Sncf,
}

impl Connector {
    /// Registers the connector, configured from the environment.
    pub fn register(self, registry: &mut Registry) {
        match self {
            Self::Sncf => sncf::register(registry, autocruise_sncf::Sncf::from_env()),
        }
    }
}
