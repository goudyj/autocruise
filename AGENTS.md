# AGENTS.md

Guidance for AI coding agents (and humans) working on this repository.

## Project

autocruise is a **local MCP server** (plus a small CLI) that gives AI assistants tools for everyday services: groceries (Carrefour, E.Leclerc, Intermarché, cross-store basket comparison), health (Doctolib), transport (SNCF, IDFM), and more later.

Status: early. The SNCF connector works; others are planned. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.

## Layout

- `crates/autocruise/`: binary. `src/server.rs` (MCP server + registry, connector-agnostic), `src/tools/<connector>.rs` (MCP tool wrappers), `src/local.rs` (in-process client used by the CLI).
- `crates/autocruise-core/`: shared time zone handling, HTTP transport (real + mock), error kinds, throttling, cache.
- `connectors/<service>/`: one crate per service (`autocruise-<service>`), MCP-agnostic, with `tests/fixtures/` and `tests/requests.rs`.
- `docs/`: principles, architecture, one page per connector.

Dependency rules: connectors never depend on `rmcp` or `reqwest` (use `autocruise_core::http`); `server.rs` never names a connector.

## Useful commands

```bash
cargo run -- tools                                   # list tools and schemas
cargo run -- call sncf_departures '{"station":"Paris Montparnasse"}'   # live call (needs SNCF_API_TOKEN)
cargo run -- mcp                                     # stdio MCP server
```

## Non-negotiable rules

Read [docs/PRINCIPLES.md](docs/PRINCIPLES.md) before writing any connector. In short:

- **Local only.** No hosted backend acting for third parties. HTTP transport binds to `127.0.0.1` by default.
- **Official APIs first.** Unofficial connectors go through the user's own visible browser session (CDP).
- **Never circumvent protections.** No CAPTCHA solvers, proxy rotation, fingerprint spoofing, or anti-bot reverse-engineering. On a challenge: stop and tell the user.
- **Human pace.** Throttle and cache every connector. No bulk extraction.
- **Confirm side effects.** Mark tools read-only vs. mutating; mutations require explicit confirmation. Never automate payment.
- **No secrets, no sensitive data on disk.** Never store passwords or export cookies. Never log health data.

## Conventions

- **English everywhere**: code, comments, docs, commit messages, issues.
- Language: **Rust** (stable). Format with `cargo fmt`, lint with `cargo clippy --all-targets -- -D warnings`, test with `cargo test`.
- Tests must **never hit live sites**. Use recorded fixtures (sanitized: no personal data, no tokens).
- Times: outputs are RFC 3339 in Europe/Paris **with offset**; inputs without offset are Europe/Paris. Use `autocruise_core::time`; never format local times by hand.
- Each connector is self-contained and can be disabled at runtime.
- Connector errors implement `autocruise_core::error::Classify`; tools return `ToolError` (JSON contract in `docs/ARCHITECTURE.md`).
- Tool names: `snake_case`, prefixed by connector when ambiguous (e.g. `leclerc_search_products`). Tool descriptions are prompts for the model: keep them precise and state side effects.
- Small, focused commits with imperative messages (`Add Leclerc store search`).

## Before finishing a change

1. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` pass.
2. No new rule from `docs/PRINCIPLES.md` is broken.
3. README / docs updated if behavior or setup changed.
