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
| User's browser over CDP | `autocruise-browser` (planned) | No API, and the site tolerates a browser driven over the DevTools protocol. |
| User's browser through the desktop | `autocruise-desktop` | No API, and the site blocks DevTools-driven browsers (SNCF Connect / DataDome). |

Each path has a transport trait with a real and a mock implementation, so
connectors are tested without network or browser.

### The desktop path: accessibility + real input, not CDP

Some sites (SNCF Connect, protected by DataDome) answer any Chrome driven over
the DevTools protocol (`--remote-debugging-port`, Playwright, Puppeteer) with
"Access is temporarily restricted", even on the user's own connection. The
same user's normal Chrome, used by hand, works. autocruise does not try to hide
CDP (that would be circumvention, see [PRINCIPLES.md](PRINCIPLES.md)); it uses
the path assistive technology uses instead:

- **Read** the page through the accessibility tree (AT-SPI over D-Bus on
  Linux; Chrome started with `--force-renderer-accessibility`). Nothing is
  injected into the page, no debugging port is open.
- **Act** with real pointer and keyboard events on the display (X11 XTest),
  paced by `autocruise_core::throttle`, like a person at the keyboard.
- **Stop** on any bot check, block page or logged-out state
  (`user_action_required`); the user handles it in the same browser.

```
connector ──▶ Driver (throttle, wait for page) ──▶ Desktop trait
                                                    ├─ LinuxDesktop: AT-SPI (read) + XTest (input)
                                                    └─ MockDesktop: recorded trees (fixtures) + recorded inputs
```

The browser must run on a display that stays up: the Gateway's managed desktop
stops when nobody watches it, and Chrome dies with it. `deploy/desktop/`
starts a persistent display (Xvfb, or Xtigervnc to look at it) with its own
D-Bus session for the user's Chrome; see
[connectors/sncf-connect.md](connectors/sncf-connect.md).

## Egress: the user's own connection (optional)

When autocruise runs on a server (VM, VPS) rather than at home, sites see a
datacenter IP. The optional **egress** container (`deploy/egress`) joins the
user's tailnet and leaves the Internet through a Tailscale exit node **at the
user's home**, so the browser uses the user's real connection. It is opt-in,
only carries browser traffic, and is the user's own network, never a
third-party proxy. See [EGRESS.md](EGRESS.md).

Egress is pure deployment: no crate knows about it, and the default setup
(running at home) does not use it.

```
Chrome ──(SOCKS5 127.0.0.1:1055)──▶ egress (tailscale) ──▶ exit node at home ──▶ Internet
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
  autocruise-desktop/    # shared, MCP-agnostic, names no service
    Desktop trait        #   tree(app), input(click/type/key/scroll)
    linux                #   LinuxDesktop: AT-SPI + XTest
    mock, record         #   MockDesktop (fixtures), RecordingDesktop (to record fixtures)
    Driver               #   throttled input, wait_until / wait_until_stable
connectors/
  sncf/                  # autocruise-sncf: official SNCF API (Navitia)
  sncf-connect/          # autocruise-sncf-connect: prices + fare hold via the user's browser (desktop path)
deploy/
  egress/compose.yaml    # optional: route the browser through the user's home (Tailscale exit node)
  desktop/               # persistent display + D-Bus for the user's Chrome (desktop path)
```

Planned: `crates/autocruise-browser` (attach to the user's Chrome over CDP, for sites that allow it),
`crates/autocruise-compare` (cross-store product matching), and connectors for
Leclerc, Carrefour, Intermarché, Doctolib…

## Dependency rules

```
autocruise (binary) ──▶ connectors ──▶ autocruise-core
        └──────────────────────────────▶ autocruise-core
```

- Connectors never depend on `rmcp`. They expose plain async methods.
- Connectors never use `reqwest` directly: they go through `HttpTransport`.
- Connectors never use `atspi`/`x11rb` directly: they go through `autocruise_desktop::Driver`.
- `autocruise-core` never names a service.
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

`user_action_required` means the user must act before a retry: log in, pass a verification or wait out a block. autocruise never does these
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
| Desktop flows | `connectors/*/tests/flow.rs` | Recorded accessibility trees + scripted transitions: inputs sent, pages read, stop on challenge |
| MCP | `crates/autocruise/src/tools/*.rs` | Tool listing, annotations, structured output and error contract over a real MCP session |

Fixtures are recorded from the live API and trimmed (no personal data, no
tokens). Desktop fixtures are accessibility trees recorded with
`RecordingDesktop` (see the `live_prices` example), trimmed, and stripped of
names and other personal data before they are committed. Live checks are manual: `autocruise call ...` with real credentials.

## Adding a connector

1. `connectors/<name>` crate following the layout above, depending on `autocruise-core`.
2. Tests: fixtures + `convert` tests + request tests with `MockTransport`.
3. `crates/autocruise/src/tools/<name>.rs`: `#[tool_router(router = <name>_router, vis = "pub(crate)")]` on `impl Autocruise`, and `register(registry, service)`.
4. One `Connector` variant in `tools/mod.rs`. Nothing else in the server changes.
5. Annotate tools: `read_only_hint = true` for reads; mutating tools require explicit confirmation (see [PRINCIPLES.md](PRINCIPLES.md)).
6. Document it in `docs/connectors/<name>.md` and the README table.
