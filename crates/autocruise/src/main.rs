//! autocruise: local MCP server (stdio) + small debugging CLI.
//!
//! - `server`: the MCP server and its connector registry.
//! - `tools`: one module per connector, exposing its MCP tools.
//! - `error`: tool error contract.
//! - `local`: in-process MCP client used by the CLI.

mod error;
mod local;
mod server;
mod tools;

use anyhow::Context;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use server::Autocruise;
use tools::Connector;

#[derive(Parser)]
#[command(
    name = "autocruise",
    version,
    about = "Put everyday life on cruise control: a local MCP server."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the MCP server over stdio (what MCP clients launch).
    Mcp(Enable),
    /// Print the available tools and their JSON schemas.
    Tools(Enable),
    /// Call one tool and print its JSON result (for testing without an AI client).
    Call {
        /// Tool name, e.g. sncf_departures.
        tool: String,
        /// Tool arguments as a JSON object.
        #[arg(default_value = "{}")]
        args: String,
        #[command(flatten)]
        enable: Enable,
    },
}

#[derive(clap::Args)]
struct Enable {
    /// Connectors to enable (comma-separated).
    #[arg(long, value_delimiter = ',', default_value = "sncf")]
    enable: Vec<Connector>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Mcp(e) => {
            let service = Autocruise::new(&e.enable)
                .serve(rmcp::transport::stdio())
                .await
                .context("failed to start the MCP server")?;
            service.waiting().await?;
        }
        Command::Tools(e) => {
            let client = local::connect(Autocruise::new(&e.enable)).await?;
            let tools = client.list_all_tools().await;
            client.cancel().await.ok();
            println!("{}", serde_json::to_string_pretty(&tools?)?);
        }
        Command::Call { tool, args, enable } => {
            let args: serde_json::Value =
                serde_json::from_str(&args).context("arguments must be valid JSON")?;
            let args = args
                .as_object()
                .cloned()
                .context("arguments must be a JSON object")?;
            let client = local::connect(Autocruise::new(&enable.enable)).await?;
            let result = client
                .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
                .await;
            client.cancel().await.ok();
            let result = result?;
            let output = local::render(&result)?;
            if result.is_error == Some(true) {
                eprintln!("{output}");
                std::process::exit(1);
            }
            println!("{output}");
        }
    }
    Ok(())
}
