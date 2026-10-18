//! MCP tools of the SNCF connector. Thin wrappers: all logic lives in
//! `autocruise-sncf`.

use autocruise_sncf::Sncf;
use autocruise_sncf::types::*;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};

use crate::error::ToolError;
use crate::server::{Autocruise, Registry};

pub fn register(registry: &mut Registry, sncf: Sncf) {
    registry.add(sncf, Autocruise::sncf_router());
}

#[tool_router(router = sncf_router, vis = "pub(crate)")]
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
    ) -> Result<Json<SearchPlacesResult>, ToolError> {
        self.service::<Sncf>()?
            .search_places(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
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
    ) -> Result<Json<NearbyStationsResult>, ToolError> {
        self.service::<Sncf>()?
            .nearby_stations(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
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
    ) -> Result<Json<PlanJourneyResult>, ToolError> {
        self.service::<Sncf>()?
            .plan_journey(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
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
    ) -> Result<Json<BoardResult>, ToolError> {
        self.service::<Sncf>()?
            .departures(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
    }

    #[tool(
        name = "sncf_arrivals",
        description = "Next train arrivals at a station (arrival board), with real-time delays and disruptions. \"direction\" is the train's final destination.",
        annotations(title = "Arrival board", read_only_hint = true, open_world_hint = true)
    )]
    async fn sncf_arrivals(
        &self,
        params: Parameters<BoardParams>,
    ) -> Result<Json<BoardResult>, ToolError> {
        self.service::<Sncf>()?
            .arrivals(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
    }

    #[tool(
        name = "sncf_train",
        description = "Details of a train by number on a given day: every stop with scheduled times, skipped stops and disruptions.",
        annotations(title = "Train details", read_only_hint = true, open_world_hint = true)
    )]
    async fn sncf_train(
        &self,
        params: Parameters<TrainParams>,
    ) -> Result<Json<TrainResult>, ToolError> {
        self.service::<Sncf>()?
            .train(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
    }

    #[tool(
        name = "sncf_disruptions",
        description = "Current and upcoming SNCF disruptions (cancellations, delays, works), for a station or network-wide for a day.",
        annotations(title = "Disruptions", read_only_hint = true, open_world_hint = true)
    )]
    async fn sncf_disruptions(
        &self,
        params: Parameters<DisruptionsParams>,
    ) -> Result<Json<DisruptionsResult>, ToolError> {
        self.service::<Sncf>()?
            .disruptions(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use autocruise_core::http::mock::MockTransport;
    use autocruise_sncf::SncfClient;
    use rmcp::model::CallToolRequestParams;
    use serde_json::{Value, json};

    use super::*;
    use crate::local::{self, LocalClient};

    const DEPARTURES: &str =
        include_str!("../../../../connectors/sncf/tests/fixtures/departures.json");

    async fn client(token: Option<&str>) -> LocalClient {
        let mock = MockTransport::new().route(
            "/stop_areas/stop_area:SNCF:87391003/departures",
            &[],
            200,
            DEPARTURES,
        );
        let sncf = Sncf::new(
            SncfClient::new(
                Arc::new(mock),
                token.map(String::from),
                "http://mock.test/sncf",
            )
            .with_min_interval(Duration::ZERO),
        );
        let mut registry = Registry::default();
        register(&mut registry, sncf);
        local::connect(registry.build()).await.unwrap()
    }

    async fn call(client: &LocalClient, tool: &str, args: Value) -> rmcp::model::CallToolResult {
        let args = args.as_object().unwrap().clone();
        client
            .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn tools_are_listed_read_only_with_output_schemas() {
        let client = client(Some("t")).await;
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 7);
        for tool in &tools {
            assert!(tool.name.starts_with("sncf_"), "{}", tool.name);
            assert!(tool.output_schema.is_some(), "{}", tool.name);
            let ann = tool.annotations.as_ref().expect("annotations");
            assert_eq!(ann.read_only_hint, Some(true), "{}", tool.name);
        }
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn call_returns_structured_content() {
        let client = client(Some("t")).await;
        let args = json!({ "station": "stop_area:SNCF:87391003", "datetime": "2026-10-19T08:00" });
        let result = call(&client, "sncf_departures", args).await;
        assert_ne!(result.is_error, Some(true));
        let out = result.structured_content.expect("structured content");
        assert_eq!(out["trains"].as_array().unwrap().len(), 4);
        assert_eq!(out["since"], "2026-10-19T08:00:00+02:00");
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn errors_follow_the_json_contract() {
        let client = client(None).await;
        let args = json!({ "station": "stop_area:SNCF:87391003" });
        let result = call(&client, "sncf_departures", args).await;
        assert_eq!(result.is_error, Some(true));
        let text = local::render(&result).unwrap();
        let err: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(err["error"]["kind"], "missing_credentials");
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn invalid_arguments_are_rejected() {
        let client = client(Some("t")).await;
        let res = client
            .call_tool(
                CallToolRequestParams::new("sncf_departures")
                    .with_arguments(json!({ "count": 3 }).as_object().unwrap().clone()),
            )
            .await;
        assert!(res.is_err() || res.unwrap().is_error == Some(true));
        client.cancel().await.unwrap();
    }
}
