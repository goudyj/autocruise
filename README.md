# autocruise

> Put everyday life on cruise control. 🚗💨

**autocruise** is a **local** [MCP](https://modelcontextprotocol.io) server (plus a small CLI) that gives AI assistants — Claude, Cursor, OpenClaw, any MCP client — tools for everyday services in France: groceries, doctor appointments, trains, and more.

It runs **on your machine**, with **your own accounts**, in **your own browser**. No cloud backend, no credential storage on someone else's server.

> ⚠️ **Status: early design.** Nothing works yet. Star/watch to follow along.

## Why

Every French daily-life service has (at best) one or two scattered, unofficial MCP servers, each with its own stack, install method and quality. autocruise aims to be **one install, many connectors**, with the same safety rails everywhere — and to enable things no single connector can do, like:

> *"Compare my basket between Carrefour, Leclerc and Intermarché."*

## Planned connectors

| Module | Services | Access | Status |
|---|---|---|---|
| `courses` | Carrefour, E.Leclerc, Intermarché Drive + **cross-store basket comparison** | User's browser session | 🧭 planned (first) |
| `sante` | Doctolib (search, slots, booking with confirmation) | User's browser session | 🧭 planned |
| `transports` | SNCF, Île-de-France Mobilités | Official open APIs | 🧭 planned |
| later | Too Good To Go, Leboncoin, Vinted, … | — | 💭 ideas |

Enable only what you need:

```bash
autocruise mcp --enable courses,transports
autocruise compare "lait demi-écrémé 1L" "nutella 750g" "pâtes 500g"
```

## Design principles

1. **Local only.** The server runs on your machine (stdio or localhost HTTP), like [mcp-grafana](https://github.com/grafana/mcp-grafana).
2. **Your session, your browser.** Sites without a public API are accessed through *your* visible Chrome session (via CDP), exactly as you would by hand. autocruise never stores your passwords.
3. **No protection bypass.** No CAPTCHA solvers, no proxy rotation, no anti-bot evasion. If a site challenges you, you solve it yourself in the browser.
4. **Human pace.** Built-in throttling and caching. AI agents loop; autocruise doesn't let them hammer anyone.
5. **Explicit confirmation** before anything that commits you: booking, cancelling, adding to a cart, paying (payment is never automated).
6. **No central data.** No hosted price database, no telemetry. Sensitive data (especially health) is never logged.

See [docs/PRINCIPLES.md](docs/PRINCIPLES.md).

## Disclaimer

autocruise is an independent open-source project. It is **not affiliated with, endorsed by, or supported by** Carrefour, E.Leclerc, Intermarché / Les Mousquetaires, Doctolib, SNCF or any other service it interacts with. All trademarks belong to their owners. Unofficial connectors may break at any time and may conflict with a service's terms of use: **use them for personal purposes, at your own risk.**

## License

[MIT](LICENSE)
