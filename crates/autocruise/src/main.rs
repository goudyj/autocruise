//! autocruise: local MCP server (stdio) + small debugging CLI.

mod server;

use anyhow::Context;
use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use server::{Autocruise, Connector};

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
            let tools = Autocruise::new(&e.enable).list_tools();
            println!("{}", serde_json::to_string_pretty(&tools)?);
        }
        Command::Call { tool, args, enable } => {
            let args: serde_json::Value =
                serde_json::from_str(&args).context("arguments must be a JSON object")?;
            match Autocruise::new(&enable.enable).call(&tool, args).await {
                Ok(value) => println!("{}", serde_json::to_string_pretty(&value)?),
                Err(e) => anyhow::bail!(e),
            }
        }
    }
    Ok(())
}
