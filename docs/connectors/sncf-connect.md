# SNCF Connect (unofficial): prices and fare hold

Train ticket prices from [SNCF Connect](https://www.sncf-connect.com), read
from **your own, logged-in Chrome**, and SNCF Connect's free fare hold
("Place an option for free"). Crate: `autocruise-sncf-connect`. Enable it with
`--enable sncf,sncf-connect`.

> Unofficial: it reads the website like a person would and may break when the
> site changes. Personal use only. See the disclaimer in the README.

## How it works, and why not CDP

SNCF Connect is protected by DataDome. A Chrome driven over the DevTools
protocol (`--remote-debugging-port`, Playwright, Puppeteer) gets "Access is
temporarily restricted". autocruise does not try to hide that (no stealth, no
fingerprint tricks: [PRINCIPLES.md](../PRINCIPLES.md)). Instead it uses your
normal Chrome the way assistive technology does:

- it **reads** pages through the accessibility tree (AT-SPI),
- it **types and clicks** with real desktop input (X11 XTest), at human pace,
- it **stops** as soon as a security check, block page or logged-out header
  appears and returns `user_action_required`. It never touches a challenge:
  you complete it (or wait it out) in the browser, then ask again.

One search fills the search form (departure, arrival, date, 2-hour time slot,
travellers), presses "See prices" and reads the first page of results: about
30 to 60 seconds. Searches run one at a time, at least 30 seconds apart, and
results are cached 5 minutes. A pre-filled search URL was tried once and does
not work: `/app/redirect?redirection_type=SEARCH&...` lands on the form with the
previous values, so the form is always driven.

Linux with X11 only for now (Xvfb, Xtigervnc or a desktop session).

## Tools

| Tool | Kind | What it does |
|---|---|---|
| `sncf_connect_prices` | read-only | `{from, to, date, time_after?, passengers?}` → trains from `time_after` (first page, about 5 to 10 trains): departure/arrival (RFC 3339, Europe/Paris), duration, train type, connections, lowest 2nd/1st class fare in EUR (`null` if not sold), seats left at that price, fully booked flag; the cheapest fare of each day around the date; route notices; `more_later`. |
| `sncf_connect_hold_fare` | **mutating** | `{from, to, date, departure_time, travel_class?, passengers?, confirm?}`. Without `confirm: true`: preview only, nothing changes. With it: adds the standard fare of that class to your basket, keeps the default seat choice, skips partner offers, answers "No, do not insure this trip", places the free option and returns SNCF Connect's confirmation text. It **never pays** (any control labelled pay/payment is refused) and stops before payment. If SNCF Connect does not offer the option for the order, the trip is removed from the basket again. |

Notes:
- Extra travellers (`passengers` > 1) are added as anonymous adults (30-59)
  without discount card; the first traveller is your account profile.
- The option is not offered on OUIGO, TER, Eurostar, non-exchangeable fares,
  and was "not applicable" for a TGV INOUI leaving the next day (checked live,
  without placing anything). The deadline is set by SNCF Connect.
- The hold flow beyond the basket ("Place an option for free" → "Confirm your
  option") follows SNCF Connect's help page and is tested with mocks only: it
  has never been run against the live site.

## Setup

1. **A display that stays up, with Chrome on it.** The Gateway's managed
   desktop stops when nobody watches it, which kills Chrome. Use the script
   (nothing is installed, everything runs as you):

   ```sh
   # Close any running Chrome first: one Chrome per profile.
   deploy/desktop/browser-desktop.sh start https://www.sncf-connect.com/en-en
   deploy/desktop/browser-desktop.sh env   # variables for autocruise
   ```

   It starts `Xvfb :50`, a private D-Bus session bus in
   `$XDG_RUNTIME_DIR/autocruise-desktop-50/`, and your normal Google Chrome
   (default profile, so your logins are kept) with
   `--force-renderer-accessibility` and no debugging port.

   To **see** that display (log in, handle a check), start it with `--vnc`
   instead: Xtigervnc on `localhost:5950` with a password you create once
   with `tigervncpasswd ~/.config/autocruise/vncpasswd`. Connect a VNC viewer
   through SSH (`ssh -L 5950:localhost:5950 host`), or point the OpenClaw
   desktop view at it (attach mode, `desktop.host.port`). Without a viewer,
   `browser-desktop.sh stop`, then open Chrome in a visible desktop, fix
   things there, close it and start the script again.

   To start it at login, a user unit (not installed for you) could be:

   ```ini
   # ~/.config/systemd/user/autocruise-desktop.service
   [Unit]
   Description=Persistent display for the user's Chrome (autocruise)
   [Service]
   Type=oneshot
   RemainAfterExit=yes
   ExecStart=%h/path/to/autocruise/deploy/desktop/browser-desktop.sh start
   ExecStop=%h/path/to/autocruise/deploy/desktop/browser-desktop.sh stop
   [Install]
   WantedBy=default.target
   ```

2. **Log in yourself** to sncf-connect.com in that Chrome, once. autocruise
   never types credentials. Use the site in English: autocruise opens
   `/en-en/` pages.

3. **Point autocruise at the display** (MCP client environment):

   ```json
   "env": {
     "AUTOCRUISE_DESKTOP_DISPLAY": ":50",
     "AUTOCRUISE_DESKTOP_DBUS": "unix:path=/run/user/1000/autocruise-desktop-50/bus"
   }
   ```

   Defaults: `DISPLAY` and `DBUS_SESSION_BUS_ADDRESS`; `AT_SPI_BUS_ADDRESS`
   is honored. Args: `["mcp", "--enable", "sncf,sncf-connect"]`.

Check without an AI client:

```sh
autocruise call sncf_connect_prices '{"from":"Paris","to":"Lyon","date":"2026-10-21","time_after":"08:00"}' --enable sncf-connect
```

## Errors

| kind | When |
|---|---|
| `user_action_required` | Security check or block page (DataDome), logged out, Chrome not running or not exposing accessibility, display unreachable. |
| `not_found` | SNCF Connect does not know the place; no train at that time; no fare on sale for that class. |
| `invalid_input` | Bad date/time, date in the past or not on sale, passengers out of 1-8, option not offered for the order. |
| `upstream` | A page did not load in time or looked unexpected. |

## Development

- `cargo run -p autocruise-desktop --example poke -- text` prints what the
  accessibility tree shows (also `click X Y`, `key ctrl+l`, `type TEXT`).
- `cargo run -p autocruise-sncf-connect --example live_prices -- '<json>' /tmp/rec`
  runs one live search and records every tree read. Trim and **sanitize**
  (your name, companions, loyalty status, e-mail) before adding fixtures.
- Tests (`tests/flow.rs`) replay recorded trees of a Paris → Lyon search with
  a mock desktop; basket pages for the fare hold are synthetic.
