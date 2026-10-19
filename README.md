# autocruise

**Put everyday life on cruise control.**

autocruise lets your AI assistant handle everyday errands for you: check your train, compare your grocery basket across supermarkets, find a doctor's appointment. It runs **locally, on your machine**, and acts with **your own accounts**.

> 🚧 **Early stage.** The SNCF connector works; the others are planned.

## What is MCP?

The [Model Context Protocol](https://modelcontextprotocol.io) (MCP) is an open standard that lets AI assistants (Claude, ChatGPT, Cursor…) use external **tools**. An MCP server exposes tools such as `sncf_departures`; the assistant decides when to call them.

autocruise is an MCP server that bundles many everyday-life tools in a single install.

## Example

> **You:** I need to be in Lyon tomorrow before 11. Which train?
>
> **Assistant** *(using autocruise)*: The direct TGV INOUI 6607 leaves Paris Gare de Lyon at 09:00 and arrives at Lyon Part-Dieu at 10:56. No disruption reported.

## Quick start

Requires [Rust](https://rustup.rs).

```bash
cargo install --git https://github.com/goudyj/autocruise autocruise
```

Get a free SNCF API key at <https://numerique.sncf.com/startup/api/>, then add autocruise to your MCP client. For Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "autocruise": {
      "command": "autocruise",
      "args": ["mcp"],
      "env": { "SNCF_API_TOKEN": "your-key" }
    }
  }
}
```

For Claude Code: `claude mcp add autocruise --env SNCF_API_TOKEN=your-key -- autocruise mcp`

Try a tool without any AI client:

```bash
SNCF_API_TOKEN=your-key autocruise call sncf_departures '{"station": "Paris Montparnasse"}'
```

## How it works

```
AI assistant ──MCP──▶ autocruise (your machine) ──▶ official APIs (SNCF, …)
                                                ├─▶ your own browser session (planned: Doctolib, supermarkets)
                                                └─▶ the official app on your own Android device (experimental)
```

- **Official APIs** are used whenever a service provides one.
- Otherwise, autocruise will work through **your already-open browser**, as you would by hand: you log in yourself, nothing is stored on a server.
- For app-only services, it drives the **official app** on an Android device you control (a local container or your phone). Nothing is disguised; see [docs/ANDROID.md](docs/ANDROID.md).
- Running autocruise on a server instead of at home? *Optionally*, browser and device traffic can leave through **your home connection** (Tailscale exit node); see [docs/EGRESS.md](docs/EGRESS.md). Not needed otherwise.
- Anything that commits you (booking, adding to a cart) **asks for confirmation first**. Payment is never automated.
- Times are always given in French time with an explicit UTC offset.

## Connectors

| Area | Services | Status |
|---|---|---|
| Transport | SNCF: stations, journeys, departures/arrivals, trains, disruptions ([details](docs/connectors/sncf.md)) | ✅ available |
| Groceries | Carrefour, E.Leclerc, Intermarché, and cross-store basket comparison | 🧭 planned |
| Health | Doctolib | 🧭 planned |

## Principles

Local only · your session, your browser · no anti-bot circumvention · human-paced requests · confirmation before side effects · no telemetry. Details in [docs/PRINCIPLES.md](docs/PRINCIPLES.md). Internals in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Disclaimer

Independent open-source project, **not affiliated with** any of the services above. Unofficial connectors may break at any time and may conflict with a service's terms of use. Use them for personal purposes, at your own risk.

## License

[MIT](LICENSE)
