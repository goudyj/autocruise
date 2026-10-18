//! In-process MCP client: the CLI goes through the exact same MCP path as a
//! real client (tool routing, argument parsing, error mapping).

use anyhow::Context;
use rmcp::model::CallToolResult;
use rmcp::service::RunningService;
use rmcp::{RoleClient, ServiceExt};

use crate::server::Autocruise;

pub type LocalClient = RunningService<RoleClient, ()>;

/// Starts `server` on an in-memory pipe and returns a connected client.
pub async fn connect(server: Autocruise) -> anyhow::Result<LocalClient> {
    let (server_io, client_io) = tokio::io::duplex(256 * 1024);
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_io)
        .await
        .context("in-process MCP handshake failed")
}

/// Pretty JSON for structured results, raw text otherwise.
pub fn render(result: &CallToolResult) -> anyhow::Result<String> {
    if let Some(structured) = &result.structured_content {
        return Ok(serde_json::to_string_pretty(structured)?);
    }
    let text: Vec<&str> = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect();
    Ok(text.join("\n"))
}
