---
name: autocruise
description: Use autocruise's MCP tools (SNCF timetables, SNCF Connect prices and fare hold) for a user, and set up or repair autocruise as an MCP server for an agent gateway.
---

# autocruise

autocruise is a local MCP server. Repository docs are the reference:
`README.md`, `docs/OPENCLAW.md` (gateway setup), `docs/connectors/sncf-connect.md`,
`docs/PRINCIPLES.md` (rules you must follow).

## Answering a train question

1. **Timetables, delays, disruptions, train details**: `sncf_*` tools (official
   API, fast). They have **no prices**.
2. **Prices**: `sncf_connect_prices {from, to, date, time_after?, time_before?,
   direct_only?, train_types?}`. It drives the user's own Chrome: one call takes
   1–3 minutes. Call it **once** per question and wait; never call it in
   parallel. Use the returned `cheapest` field; mention `more_later` if true.
   For "high-speed only" pass `train_types: ["tgv_inoui","ouigo"]`.
3. Present results as a short table (departure → arrival, stations, train,
   2nd/1st class price). Point out unusual stations (e.g. Marne-la-Vallée,
   Perrache) and very long journeys (TER).
4. **Holding a fare** (`sncf_connect_hold_fare`) changes the user's basket:
   - call it **without** `confirm` first and show the preview;
   - call it with `confirm: true` only after the user explicitly agrees;
   - if two trains leave at the same time, pass the `train_number` returned by
     `sncf_connect_prices`;
   - it never pays; the user pays later in SNCF Connect. Not offered on OUIGO,
     TER, Eurostar, non-exchangeable fares, or close to departure.

## Errors

| kind | What to do |
|---|---|
| `user_action_required` | Relay the message. The user must log in or pass a security check **themselves** in the browser. Never try to work around it, never retry in a loop. |
| `missing_credentials` / `unauthorized` | `SNCF_API_TOKEN` missing or wrong in the server environment (see setup). |
| `rate_limited` | Another search holds the browser; retry once a minute later. |
| `not_found` | Unknown place or no train: ask for a fuller station name or another time. |

## Setup and repair (operator)

Follow `docs/OPENCLAW.md`. Check in order:

1. Binary installed and current: `cargo install --path crates/autocruise --locked`
   after every update (the gateway runs the installed binary).
2. Browser display (SNCF Connect only): `systemctl --user status
   autocruise-desktop.service` or `deploy/desktop/browser-desktop.sh status`
   shows x, dbus and chrome running; `browser-desktop.sh env` prints the two
   `AUTOCRUISE_DESKTOP_*` variables the server needs.
3. Credentials: the server command is `deploy/autocruise-env.sh`, which loads
   `~/.config/autocruise/env` (mode 600). The user stores the token with
   `deploy/autocruise-env.sh --set SNCF_API_TOKEN` (hidden prompt). Never ask
   for a key in chat, never write it into gateway config or the repository.
   Do not send shell one-liners containing `NAME=value` through a chat that
   masks secrets: point to `--set` instead.
4. MCP registration with a request timeout of at least 300 s (whole-day
   searches exceed the usual 60 s). Probe it: 9 tools with
   `--enable sncf,sncf-connect`.
5. Live check, once: `deploy/autocruise-env.sh call sncf_departures
   '{"station":"Paris Gare de Lyon"}' --enable sncf`.

On `user_action_required` from SNCF Connect, first read the active page through
accessibility (`cargo run -p autocruise-desktop --example poke -- text`, with
the `env` variables exported) to tell a real check from a false alarm. A real
check or block page: stop and hand it to the user.

## Never

- Drive SNCF Connect over CDP, Playwright or Puppeteer, or hide automation.
- Solve, slide or click a CAPTCHA or DataDome check.
- Pay, or confirm a fare hold without the user's explicit yes.
