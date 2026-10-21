# autocruise

**Put everyday life on cruise control.**

autocruise gives your AI assistant tools for everyday errands: find the cheapest
train, check delays, hold a fare… and later compare your grocery basket or
find a doctor's appointment. It runs **on your own machine** and acts with
**your own accounts**. Nothing goes through a third-party server.

> 🚧 **Early stage.** Two SNCF connectors work today; the others are planned.

## Example

> **You** *(on Telegram, or in Claude…)*: Cheapest train Paris → Lyon next Monday after 2 pm?
>
> **Assistant** *(using autocruise)*: The cheapest is the OUIGO at 20:58 from
> Marne-la-Vallée to Lyon Perrache, €35. From Gare de Lyon, the OUIGO at 19:26
> is €49, and the TGV INOUI at 18:26 is €60 (€69 in first). Want me to hold
> the 18:26 fare?

## The ideas in 2 minutes

### What is MCP?

An AI assistant can only talk, unless you give it **tools**. The
[Model Context Protocol](https://modelcontextprotocol.io) (MCP) is the
standard way to do that: a small program (an *MCP server*) lists tools such as
`sncf_connect_prices` with a description and parameters; the assistant reads
the list and decides by itself when to call which tool.

autocruise is one MCP server that bundles many everyday-life tools. Any MCP
client can use it: Claude Desktop, Claude Code, Cursor, or a self-hosted
gateway such as [OpenClaw](https://docs.openclaw.ai) that connects it to
Telegram, WhatsApp, etc.

```
You ──chat──▶ AI assistant ──MCP──▶ autocruise (your machine) ──▶ the services
```

### How autocruise reaches a service

For each service it uses the most legitimate way available:

| Way | When | Example |
|---|---|---|
| **Official API** | The service publishes one. Fast and reliable. | SNCF timetables, delays, disruptions |
| **Your own browser** | No API, only a website. autocruise uses *your* normal Chrome, where *you* are logged in, like a person would. | SNCF Connect prices and fare hold |

**Why not a "robot browser"?** Many sites (SNCF Connect included) are
protected by anti-bot systems such as DataDome. Tools like Playwright or
Puppeteer drive Chrome through a debugging port, which these systems detect
and block. autocruise does not try to hide automation. It uses your ordinary
Chrome the way **assistive technology** (screen readers) does:

- it **reads** the page through the accessibility tree (on Linux: AT-SPI),
  which describes every button, heading and price as text;
- it **clicks and types** with real keyboard and mouse events, at a human pace;
- it **stops and tells you** as soon as a security check, a block page or a
  logged-out screen appears. It never solves a CAPTCHA: you do it yourself,
  then ask again.

### Safety rules

- Anything that commits you (holding a fare, later booking) **needs explicit
  confirmation** and shows a preview first. **Payment is never automated.**
- No passwords stored. Credentials such as API keys stay in your own files.
- Human pace, one search at a time, short cache. No bulk scraping.
- Times are always French time with an explicit offset (`2026-10-26T20:58:00+01:00`).

Full rules: [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## Connectors

| Area | Service | Tools | Status |
|---|---|---|---|
| Transport | **SNCF** (official API): stations, journeys, departures/arrivals with real-time delays, train details, disruptions. [Details](docs/connectors/sncf.md) | `sncf_*` (7, read-only) | ✅ available |
| Transport | **SNCF Connect** (unofficial, your Chrome): ticket prices for a whole day with filters (direct, high-speed…), free fare hold. [Details](docs/connectors/sncf-connect.md) | `sncf_connect_prices`, `sncf_connect_hold_fare` | 🧪 experimental, Linux/X11 |
| Groceries | Carrefour, E.Leclerc, Intermarché, basket comparison | | 🧭 planned |
| Health | Doctolib | | 🧭 planned |

The official SNCF API has **no prices**: prices come from SNCF Connect.

## Quick start

Requires [Rust](https://rustup.rs).

```bash
cargo install --git https://github.com/goudyj/autocruise autocruise
```

### 1. Official SNCF API (timetables, delays)

Get a free key at <https://numerique.sncf.com/startup/api/>, then add
autocruise to your MCP client. Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "autocruise": {
      "command": "autocruise",
      "args": ["mcp", "--enable", "sncf"],
      "env": { "SNCF_API_TOKEN": "your-key" }
    }
  }
}
```

Claude Code: `claude mcp add autocruise --env SNCF_API_TOKEN=your-key -- autocruise mcp --enable sncf`

### 2. SNCF Connect (prices), optional

Needs Linux with X11 and Google Chrome. In short: run your Chrome on a
display that stays up, log in to sncf-connect.com yourself once, and enable
the connector with `--enable sncf,sncf-connect`. A script does the display
part, and can install it as a service:

```bash
deploy/desktop/browser-desktop.sh install-service --vnc
```

Step by step: [docs/connectors/sncf-connect.md](docs/connectors/sncf-connect.md).

### Try a tool without any AI client

```bash
autocruise tools --enable sncf,sncf-connect          # list tools and their schemas
SNCF_API_TOKEN=your-key autocruise call sncf_departures '{"station": "Paris Montparnasse"}'
autocruise call sncf_connect_prices '{"from":"Paris","to":"Lyon","date":"2026-10-26","time_after":"14:00"}' --enable sncf-connect
```

## Use it from Telegram (OpenClaw)

[OpenClaw](https://docs.openclaw.ai) is a self-hosted gateway that links an AI
assistant to chat apps. Register autocruise as one of its MCP servers and the
tools work from Telegram, the web UI, etc. Full guide, including the browser
service, timeouts, credentials and troubleshooting:
**[docs/OPENCLAW.md](docs/OPENCLAW.md)**.

Running autocruise on a server rather than at home? Sites see a datacenter IP.
You can *optionally* send the browser's traffic through your own home
connection with a Tailscale exit node: [docs/EGRESS.md](docs/EGRESS.md).

## Documentation

| Document | What's inside |
|---|---|
| [docs/PRINCIPLES.md](docs/PRINCIPLES.md) | The non-negotiable rules (local only, no circumvention, confirmation, privacy) |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, access paths, error contract, testing |
| [docs/OPENCLAW.md](docs/OPENCLAW.md) | Running autocruise for an OpenClaw Gateway |
| [docs/EGRESS.md](docs/EGRESS.md) | Optional: browser traffic through your home connection |
| [docs/connectors/](docs/connectors/) | One page per connector |
| [AGENTS.md](AGENTS.md) | Guide for contributors and coding agents |

## Disclaimer

Independent open-source project, **not affiliated with** SNCF or any other
service above. Unofficial connectors may break at any time and may conflict
with a service's terms of use (your account could be restricted). Use them for
personal purposes, at your own risk.

## License

[MIT](LICENSE)
