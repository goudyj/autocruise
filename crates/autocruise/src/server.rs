//! MCP server. It knows no connector: connectors register a service object
//! and their tool router in a [`Registry`] (see `crate::tools`).

use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool_handler};

use crate::error::ToolError;
use crate::tools::Connector;

const INSTRUCTIONS: &str = "autocruise: everyday-life tools for France, running locally. \
Times are Europe/Paris: outputs carry an explicit UTC offset; inputs without an offset are read as Europe/Paris local time. \
Failed calls return {\"error\": {\"kind\", \"message\"}}; kinds: missing_credentials, unauthorized, rate_limited, invalid_input, not_found, upstream, network, user_action_required, internal. \
On user_action_required, relay the message to the user and wait for them: never try to work around it. \
SNCF tools (sncf_*) give stations, journeys, departures/arrivals with real-time delays, train details and disruptions. \
They cannot give prices or book tickets.";

type Services = HashMap<TypeId, Arc<dyn Any + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct Autocruise {
    services: Arc<Services>,
    tool_router: ToolRouter<Self>,
}

impl Autocruise {
    /// Server with the given connectors, configured from the environment.
    pub fn new(enabled: &[Connector]) -> Self {
        let mut registry = Registry::default();
        for connector in enabled {
            connector.register(&mut registry);
        }
        registry.build()
    }

    /// The service object a connector registered, for use in its tools.
    pub(crate) fn service<T: Any + Send + Sync>(&self) -> Result<&T, ToolError> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|s| s.downcast_ref::<T>())
            .ok_or_else(|| ToolError::internal(format!("{} is not registered", type_name::<T>())))
    }
}

/// Collects connector services and tool routers, then builds the server.
#[derive(Default)]
pub struct Registry {
    services: Services,
    router: ToolRouter<Autocruise>,
}

impl Registry {
    pub fn add<T: Any + Send + Sync>(&mut self, service: T, router: ToolRouter<Autocruise>) {
        self.services.insert(TypeId::of::<T>(), Arc::new(service));
        self.router.merge(router);
    }

    pub fn build(self) -> Autocruise {
        Autocruise {
            services: Arc::new(self.services),
            tool_router: self.router,
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Autocruise {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("autocruise", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local;

    #[tokio::test]
    async fn no_connector_no_tools() {
        let client = local::connect(Autocruise::new(&[])).await.unwrap();
        assert!(client.list_all_tools().await.unwrap().is_empty());
        client.cancel().await.unwrap();
    }

    #[test]
    fn missing_service_is_an_internal_error() {
        let server = Registry::default().build();
        let err = server.service::<String>().unwrap_err();
        assert_eq!(err.kind, autocruise_core::error::ErrorKind::Internal);
    }
}
