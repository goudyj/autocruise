//! MCP server: exposes every enabled connector's tools.

use std::sync::Arc;

use autocruise_sncf::Sncf;
use autocruise_sncf::types::*;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig, Tool};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde_json::Value;

const INSTRUCTIONS: &str = "autocruise: everyday-life tools for France, running locally. \
Times are Europe/Paris: outputs carry an explicit UTC offset; inputs without an offset are read as Europe/Paris local time. \
SNCF tools (sncf_*) give stations, journeys, departures/arrivals with real-time delays, train details and disruptions. \
They cannot give prices or book tickets.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Connector {
    /// SNCF trains (official API, needs SNCF_API_TOKEN).
    Sncf,
}

#[derive(Debug, Clone)]
pub struct Autocruise {
    sncf: Arc<Sncf>,
    tool_router: ToolRouter<Self>,
}

impl Autocruise {
    pub fn new(enabled: &[Connector]) -> Self {
        let mut tool_router = ToolRouter::new();
        if enabled.contains(&Connector::Sncf) {
            tool_router += Self::sncf_router();
        }
        Self {
            sncf: Arc::new(Sncf::from_env()),
            tool_router,
        }
    }

    pub fn list_tools(&self) -> Vec<Tool> {
        self.tool_router.list_all()
    }

    /// Direct dispatch used by the `call` CLI command.
    pub async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        if !self.tool_router.has_route(name) {
            return Err(format!("unknown or disabled tool {name:?}"));
        }
        fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
            serde_json::from_value(v).map_err(|e| format!("invalid arguments: {e}"))
        }
        fn out<T: serde::Serialize>(
            r: Result<T, autocruise_sncf::SncfError>,
        ) -> Result<Value, String> {
            r.map_err(|e| e.to_string())
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        }
        let s = &self.sncf;
        match name {
            "sncf_search_places" => out(s.search_places(parse(args)?).await),
            "sncf_nearby_stations" => out(s.nearby_stations(parse(args)?).await),
            "sncf_plan_journey" => out(s.plan_journey(parse(args)?).await),
            "sncf_departures" => out(s.departures(parse(args)?).await),
            "sncf_arrivals" => out(s.arrivals(parse(args)?).await),
            "sncf_train" => out(s.train(parse(args)?).await),
            "sncf_disruptions" => out(s.disruptions(parse(args)?).await),
            _ => Err(format!("tool {name:?} has no CLI dispatch")),
        }
    }
}

#[tool_router(router = sncf_router)]
impl Autocruise {
    #[tool(
        name = "sncf_search_places",
        description = "Find SNCF train stations (and optionally cities, addresses, points of interest) by name. Returns ids usable by the other sncf_* tools.",
        annotations(
            title = "Search stations",
            read_only_hint = true,
            open_world_hint = true
        )
    )]
    async fn sncf_search_places(
        &self,
        params: Parameters<SearchPlacesParams>,
    ) -> Result<Json<SearchPlacesResult>, String> {
        self.sncf
            .search_places(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        name = "sncf_nearby_stations",
        description = "List SNCF train stations near a latitude/longitude, closest first.",
        annotations(
            title = "Nearby stations",
            read_only_hint = true,
            open_world_hint = true
        )
    )]
    async fn sncf_nearby_stations(
        &self,
        params: Parameters<NearbyStationsParams>,
    ) -> Result<Json<NearbyStationsResult>, String> {
        self.sncf
            .nearby_stations(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        name = "sncf_plan_journey",
        description = "Plan a train journey in France between two places (stations, cities or coordinates), departing after or arriving before a time. Includes transfers, real-time delays and disruptions. No prices, no booking.",
        annotations(
            title = "Plan a train journey",
            read_only_hint = true,
            open_world_hint = true
        )
    )]
    async fn sncf_plan_journey(
        &self,
        params: Parameters<PlanJourneyParams>,
    ) -> Result<Json<PlanJourneyResult>, String> {
        self.sncf
            .plan_journey(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        name = "sncf_departures",
        description = "Next train departures from a station (departure board), with real-time delays and disruptions. Cancelled trains may be missing from the board: check sncf_disruptions for the station.",
        annotations(
            title = "Departure board",
            read_only_hint = true,
            open_world_hint = true
        )
    )]
    async fn sncf_departures(
        &self,
        params: Parameters<BoardParams>,
    ) -> Result<Json<BoardResult>, String> {
        self.sncf
            .departures(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        name = "sncf_arrivals",
        description = "Next train arrivals at a station (arrival board), with real-time delays and disruptions. \"direction\" is the train's final destination.",
        annotations(title = "Arrival board", read_only_hint = true, open_world_hint = true)
    )]
    async fn sncf_arrivals(
        &self,
        params: Parameters<BoardParams>,
    ) -> Result<Json<BoardResult>, String> {
        self.sncf
            .arrivals(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        name = "sncf_train",
        description = "Details of a train by number on a given day: every stop with scheduled times, skipped stops and disruptions.",
        annotations(title = "Train details", read_only_hint = true, open_world_hint = true)
    )]
    async fn sncf_train(
        &self,
        params: Parameters<TrainParams>,
    ) -> Result<Json<TrainResult>, String> {
        self.sncf
            .train(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
    }

    #[tool(
        name = "sncf_disruptions",
        description = "Current and upcoming SNCF disruptions (cancellations, delays, works), for a station or network-wide for a day.",
        annotations(title = "Disruptions", read_only_hint = true, open_world_hint = true)
    )]
    async fn sncf_disruptions(
        &self,
        params: Parameters<DisruptionsParams>,
    ) -> Result<Json<DisruptionsResult>, String> {
        self.sncf
            .disruptions(params.0)
            .await
            .map(Json)
            .map_err(|e| e.to_string())
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

    #[test]
    fn sncf_tools_are_registered_and_read_only() {
        let server = Autocruise::new(&[Connector::Sncf]);
        let tools = server.list_tools();
        assert_eq!(tools.len(), 7);
        for tool in &tools {
            assert!(tool.name.starts_with("sncf_"), "{}", tool.name);
            let ann = tool.annotations.as_ref().expect("annotations");
            assert_eq!(ann.read_only_hint, Some(true), "{}", tool.name);
        }
    }

    #[test]
    fn disabled_connector_exposes_nothing() {
        assert!(Autocruise::new(&[]).list_tools().is_empty());
    }

    #[tokio::test]
    async fn every_tool_has_a_cli_dispatch() {
        let server = Autocruise::new(&[Connector::Sncf]);
        for tool in server.list_tools() {
            // Invalid arguments must fail on parsing, before any network call.
            let err = server.call(&tool.name, Value::from(42)).await.unwrap_err();
            assert!(err.starts_with("invalid arguments"), "{}: {err}", tool.name);
        }
    }
}
