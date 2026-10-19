# Architecture

autocruise is a single binary that runs **on the user's machine** and speaks
MCP over stdio. Each service lives in its own **connector** crate, which knows
nothing about MCP.

## Access paths

A connector reaches its service through exactly one of these, in order of preference
(see [PRINCIPLES.md](PRINCIPLES.md)):

| Path | Building block | Used when |
|---|---|---|
| Official API | `autocruise_core::http` | The service publishes an API (SNCF…). |
| User's browser | `autocruise-browser` (planned, CDP) | No API, but a usable website. |
| User's app | `autocruise-android` (adb) | Only a mobile app. Official app on a device the user controls ([ANDROID.md](ANDROID.md)). |

Each path has a transport trait with a real and a mock implementation, so
connectors are tested without network or device.

## Egress: the user's own connection

When autocruise runs on a server (VM, VPS) rather than at home, sites see a
datacenter IP. The optional **egress** container (`deploy/egress`) joins the
user's tailnet and leaves the Internet through a Tailscale exit node **at the
user's home**, so the browser and the Android device use the user's real
connection. It is opt-in, only carries the browser and device traffic, and
is the user's own network, never a third-party proxy. See [EGRESS.md](EGRESS.md).

```
redroid ──(shares egress network namespace)──┐
Chrome  ──(SOCKS5 127.0.0.1:1055)────────────┤──▶ egress (tailscale) ──▶ exit node at home ──▶ Internet
```

```
crates/
  autocruise/            # binary
    src/server.rs        #   MCP server + Registry (knows no connector)
    src/tools/mod.rs     #   Connector enum: which connectors exist
    src/tools/sncf.rs    #   SNCF MCP tools (thin wrappers) + register()
    src/error.rs         #   tool error contract
    src/local.rs         #   in-process MCP client, used by the CLI
  autocruise-core/       # shared, MCP-agnostic
    time                 #   Europe/Paris rules
    http                 #   HttpTransport trait, ReqwestTransport, MockTransport
    error                #   ErrorKind + Classify
    throttle, cache
  autocruise-android/    # drive the user's Android apps over adb
    adb                  #   AdbTransport trait, CommandAdb
    mock                 #   MockAdb (canned outputs, recorded commands)
    ui                   #   UiTree from uiautomator dumps, By selectors
    device               #   Device: launch, read screen, wait, tap, type, swipe
connectors/
  sncf/                  # autocruise-sncf: official SNCF API (Navitia)
deploy/
  android/compose.yaml   # local redroid device (adb on 127.0.0.1:5555)
  egress/compose.yaml    # optional: route device + browser through the user's home (Tailscale exit node)
```

Planned: `crates/autocruise-browser` (attach to the user's Chrome over CDP),
`crates/autocruise-compare` (cross-store product matching), and connectors for
Leclerc, Carrefour, Intermarché, Doctolib…

## Dependency rules

```
autocruise (binary) ──▶ connectors ──▶ autocruise-android ──▶ autocruise-core
        │                    └──────────────────────────────▶ autocruise-core
        └───────────────────────────────────────────────────▶ autocruise-core
```

- Connectors never depend on `rmcp`. They expose plain async methods.
- Connectors never use `reqwest` directly: they go through `HttpTransport`.
- App connectors never spawn `adb` themselves: they go through `autocruise_android::Device`.
- Building-block crates (`autocruise-core`, `autocruise-android`) never name a service.
- The server core (`server.rs`) never names a connector.

## Request flow

```
MCP client ──stdio──▶ Autocruise (rmcp ToolRouter)
                          │  tools/<connector>.rs: parse params, fetch service, map errors
                          ▼
                      connector service (e.g. Sncf)
                          │  resolve inputs, build requests
                          ├─▶ client: throttle ─▶ cache? ─▶ HttpTransport ─▶ service API
                          ▼
                      convert: raw API types ─▶ tool output types
```

The CLI (`autocruise tools`, `autocruise call`) starts the same server on an
in-memory pipe and talks MCP to it (`local.rs`): there is a single routing path.

## Connector crate layout

- `client.rs`: URL building, auth, HTTP status → error mapping, throttle, cache.
- `navitia.rs` (raw types): lenient serde structs (`#[serde(default)]`) for the subset used.
- `types.rs`: tool params and outputs (`JsonSchema`). Field docs are read by the model.
- `convert.rs`: pure functions raw → output.
- `resolve.rs` (when relevant): user input → service identifiers.
- `service.rs`: the service struct, one async method per tool.
- `error.rs`: detailed error enum implementing `Classify`.

## Errors

Failed tool calls return `isError: true` with one text block:

```json
{"error": {"kind": "not_found", "message": "no station or place matches \"Zzz\"; try sncf_search_places"}}
```

`kind` is one of `missing_credentials`, `unauthorized`, `rate_limited`,
`invalid_input`, `not_found`, `upstream`, `network`, `user_action_required`,
`internal` (stable contract, `autocruise_core::error::ErrorKind`).

`user_action_required` means the user must act before a retry: log in, install
the app, pass a verification or wait out a block. autocruise never does these
for the user, and the server instructions tell the assistant not to work around them. Unknown tools and malformed
arguments are MCP protocol errors.

## Time zones

French services generally return local Europe/Paris times **without offset**.
autocruise normalizes everything:

- **Outputs**: RFC 3339 in Europe/Paris with explicit offset, e.g. `2026-10-19T08:30:00+02:00`.
- **Inputs**: ISO 8601. With an offset (`Z`, `+02:00`) it is honored; without one it is Europe/Paris local time.
- **To the API**: whatever the API documents. SNCF requests are sent in UTC (`20261019T063000Z`), which it converts itself.
- **DST**: ambiguous local times (autumn) resolve to the earliest instant; nonexistent ones (spring) are rejected.

## Testing

No test touches the network.

| Level | Where | What it proves |
|---|---|---|
| Conversion | `connectors/*/src/convert.rs` | Recorded API responses → tool outputs |
| Requests | `connectors/*/tests/requests.rs` | Exact requests sent (path, query, auth, UTC times), error classification, cache |
| MCP | `crates/autocruise/src/tools/*.rs` | Tool listing, annotations, structured output and error contract over a real MCP session |
| App flows | app connectors, with `MockAdb` + recorded UI dumps | Screens → outputs, taps and text sent, block screens → `user_action_required` |

Fixtures are recorded from the live API and trimmed (no personal data, no
tokens). Live checks are manual: `autocruise call ...` with real credentials.

## Adding a connector

1. `connectors/<name>` crate following the layout above, depending on `autocruise-core` (plus `autocruise-android` for app connectors). App connectors replace `client.rs` with `screens.rs` (selectors, screen detection) and `flows.rs` (navigation); see [ANDROID.md](ANDROID.md).
2. Tests: fixtures + `convert` tests + request tests with `MockTransport`.
3. `crates/autocruise/src/tools/<name>.rs`: `#[tool_router(router = <name>_router, vis = "pub(crate)")]` on `impl Autocruise`, and `register(registry, service)`.
4. One `Connector` variant in `tools/mod.rs`. Nothing else in the server changes.
5. Annotate tools: `read_only_hint = true` for reads; mutating tools require explicit confirmation (see [PRINCIPLES.md](PRINCIPLES.md)).
6. Document it in `docs/connectors/<name>.md` and the README table.
