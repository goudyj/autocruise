//! MCP tools of the SNCF Connect connector (unofficial). Thin wrappers: all
//! logic lives in `autocruise-sncf-connect`.

use autocruise_sncf_connect::SncfConnect;
use autocruise_sncf_connect::types::*;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};

use crate::error::ToolError;
use crate::server::{Autocruise, Registry};

pub fn register(registry: &mut Registry, service: SncfConnect) {
    registry.add(service, Autocruise::sncf_connect_router());
}

#[tool_router(router = sncf_connect_router, vis = "pub(crate)")]
impl Autocruise {
    #[tool(
        name = "sncf_connect_prices",
        description = "Train ticket prices from SNCF Connect (unofficial) for one day: trains with departure/arrival, duration, train type, connections, lowest 2nd and 1st class fares in EUR, seats left at that price, plus the cheapest fare of each day around the date. Works by driving the user's own logged-in Chrome on their desktop (reads the page through accessibility, types and clicks like a person): slow (about 30 to 60 seconds), one search at a time, results cached 5 minutes. Read-only: books nothing. If SNCF Connect shows a security check or the user is logged out, it stops and returns user_action_required: tell the user to handle it in their browser.",
        annotations(
            title = "SNCF Connect prices",
            read_only_hint = true,
            open_world_hint = true
        )
    )]
    async fn sncf_connect_prices(
        &self,
        params: Parameters<PricesParams>,
    ) -> Result<Json<PricesResult>, ToolError> {
        self.service::<SncfConnect>()?
            .prices(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
    }

    #[tool(
        name = "sncf_connect_hold_fare",
        description = "Hold a train fare with SNCF Connect's free option (\"Place an option for free\"): the price and seat are kept until a deadline set by SNCF Connect, without paying. Not offered on OUIGO, TER, Eurostar or non-exchangeable fares, nor close to departure. MUTATING: with confirm=true it adds the trip to the basket of the user's SNCF Connect account in their own logged-in browser and places the option; without confirm it changes nothing and returns a preview. Always show the preview to the user and get their explicit agreement before calling with confirm=true. Never pays and never buys insurance; the user pays later in SNCF Connect. Slow (about a minute).",
        annotations(
            title = "Hold an SNCF Connect fare",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn sncf_connect_hold_fare(
        &self,
        params: Parameters<HoldFareParams>,
    ) -> Result<Json<HoldFareResult>, ToolError> {
        self.service::<SncfConnect>()?
            .hold_fare(params.0)
            .await
            .map(Json)
            .map_err(ToolError::from_err)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use autocruise_desktop::mock::MockDesktop;
    use autocruise_desktop::{Node, Rect};
    use autocruise_sncf_connect::Timing;
    use rmcp::model::CallToolRequestParams;
    use serde_json::{Value, json};

    use super::*;
    use crate::local::{self, LocalClient};

    fn blocked() -> Node {
        Node::new("application", "Google Chrome").with_children(vec![
            Node::new("entry", "Address and search bar").with_bounds(Rect::new(158, 51, 1578, 24)),
            Node::new("document web", "sncf-connect.com").with_children(vec![Node::new(
                "static",
                "Access is temporarily restricted",
            )]),
        ])
    }

    async fn client(mock: Arc<MockDesktop>) -> LocalClient {
        let service = SncfConnect::with_timing(mock, Timing::default());
        let mut registry = Registry::default();
        register(&mut registry, service);
        local::connect(registry.build()).await.unwrap()
    }

    #[tokio::test]
    async fn tools_and_annotations() {
        let client = client(Arc::new(MockDesktop::new(vec![blocked()]))).await;
        let tools = client.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        let prices = tools
            .iter()
            .find(|t| t.name == "sncf_connect_prices")
            .unwrap();
        let ann = prices.annotations.as_ref().unwrap();
        assert_eq!(ann.read_only_hint, Some(true));
        assert!(prices.output_schema.is_some());
        let hold = tools
            .iter()
            .find(|t| t.name == "sncf_connect_hold_fare")
            .unwrap();
        let ann = hold.annotations.as_ref().unwrap();
        assert_eq!(ann.read_only_hint, Some(false));
        assert_eq!(ann.destructive_hint, Some(true));
        let schema = serde_json::to_value(&hold.input_schema).unwrap();
        assert!(schema["properties"]["confirm"].is_object());
        client.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn a_challenge_is_user_action_required() {
        let mock = Arc::new(MockDesktop::new(vec![blocked()]));
        let client = client(mock.clone()).await;
        let tomorrow = (autocruise_core::time::now().date_naive() + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let args = json!({ "from": "Paris", "to": "Lyon", "date": tomorrow });
        let result = client
            .call_tool(
                CallToolRequestParams::new("sncf_connect_prices")
                    .with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        let err: Value = serde_json::from_str(&local::render(&result).unwrap()).unwrap();
        assert_eq!(err["error"]["kind"], "user_action_required");
        assert!(mock.inputs().is_empty());
        client.cancel().await.unwrap();
    }
}
