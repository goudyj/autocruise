# Architecture

autocruise is a single binary that runs **on the user's machine** and speaks
MCP over stdio. Each service lives in its own **connector** crate.

```
crates/
  autocruise/        # binary: MCP server (rmcp, stdio) + debugging CLI
  autocruise-core/   # shared: Europe/Paris time handling, throttling, cache
connectors/
  sncf/              # autocruise-sncf: official SNCF API (Navitia)
```

Planned:

```
crates/autocruise-browser/   # attach to the user's Chrome over CDP, run fetch() in the page
crates/autocruise-compare/   # cross-store product matching (EAN, then price per unit)
connectors/leclerc, carrefour, intermarche, doctolib, ...
```

## Request flow

```
MCP client ──stdio──▶ autocruise (server.rs)
                          │  #[tool] wrapper: parse params, call connector, wrap Json
                          ▼
                      connector (e.g. autocruise-sncf)
                          │  resolve inputs, build request
                          ├─▶ client: throttle ─▶ cache? ─▶ HTTPS ─▶ service API
                          ▼
                      convert: raw API types ─▶ tool output types
```

## Responsibilities

**`autocruise` (binary)**
- Registers the tools of every enabled connector (`--enable sncf,...`).
- Tool wrappers are thin: no business logic.
- `autocruise call <tool> '<json>'` runs a tool without an AI client.
- `autocruise tools` prints tool definitions and JSON schemas.

**`autocruise-core`**
- `time`: France time zone rules (see below).
- `throttle`: serializes requests with a minimum interval per connector.
- `cache`: in-memory TTL cache for reference data. Never written to disk.

**Connector crate**
- `client.rs`: HTTP, auth, error mapping. Credentials come from environment variables.
- `navitia.rs` / raw types: lenient serde structs (`#[serde(default)]`) for the subset used.
- `types.rs`: tool params and outputs (`JsonSchema`). Field docs are read by the model.
- `convert.rs`: pure functions raw → output, unit-tested against recorded fixtures.
- `lib.rs`: the connector service, one async method per tool.

## Time zones

French services generally return local Europe/Paris times **without offset**.
autocruise normalizes everything:

- **Outputs**: RFC 3339 in Europe/Paris with explicit offset, e.g. `2026-10-19T08:30:00+02:00`.
- **Inputs**: ISO 8601. With an offset (`Z`, `+02:00`) it is honored; without one it is Europe/Paris local time.
- **To the API**: whatever the API documents. SNCF requests are sent in UTC (`20261019T063000Z`), which it converts itself.
- **DST**: ambiguous local times (autumn) resolve to the earliest instant; nonexistent ones (spring) are rejected.

## Errors

Connector errors are returned as MCP tool errors (`isError: true`) with an
actionable message (missing key, invalid input, nothing found). The server
never crashes on a bad tool call.

## Testing

- Unit tests use **recorded, trimmed fixtures** (`connectors/*/tests/fixtures`). No live calls in `cargo test`.
- Live checks are manual: `autocruise call ...` with real credentials.

## Adding a connector

1. `cargo new --lib connectors/<name>` and add it to the workspace dependencies.
2. Follow the crate layout above; record sanitized fixtures; test `convert`.
3. Add a `Connector` variant and a `#[tool_router(router = <name>_router)]` block in `crates/autocruise/src/server.rs`, plus the CLI dispatch.
4. Annotate tools: `read_only_hint = true` for reads; mutating tools must require explicit confirmation (see [PRINCIPLES.md](PRINCIPLES.md)).
5. Document it in `docs/connectors/<name>.md` and the README table.
