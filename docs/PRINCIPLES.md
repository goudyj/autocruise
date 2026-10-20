# autocruise — Engineering & legal principles

These are hard rules for every connector. A PR that breaks one of them will not be merged.

## 1. Local execution
- The server runs on the user's machine (stdio, or HTTP bound to `127.0.0.1` by default).
- No hosted autocruise backend that acts on behalf of third parties.
- When it runs on the user's own server, browser traffic may leave through the **user's own home connection** (a Tailscale exit node they run; see [EGRESS.md](EGRESS.md)). Never through someone else's network.

## 2. Official APIs first
- If a service offers a public/official API (SNCF, IDFM, data.gouv…), use it.
- Unofficial connectors live under clearly labelled modules and say so in their tool descriptions.

## 3. User session, visible browser
- Sites without an API are accessed through the user's own Chrome session: over CDP when the site accepts it, otherwise the way assistive technology works (accessibility tree to read, real desktop input to act; see [ARCHITECTURE.md](ARCHITECTURE.md)).
- Login, store selection and any bot challenge are handled **by the user**, in the browser.
- No password storage. No cookie-jar export. No debugging port on the user's main profile.

## 4. No circumvention
- No CAPTCHA-solving services, no residential or rotating proxies (third-party IPs), no fingerprint spoofing, no reverse-engineering of anti-bot challenges.
- No hiding automation: no "stealth" patches, no masking of CDP. If a site refuses automated browsers, use the desktop path (the user's real browser, real input, human pace) or nothing.
- Never interact with a challenge: no click, slide, typing or waiting it out in a loop on a CAPTCHA or verification page.
- If a challenge or block appears, stop and tell the user (`user_action_required`). Never retry in a loop.

## 5. Human pace
- Per-connector throttling (serialized requests + delay) and short-lived local cache.
- No bulk extraction of catalogues, directories or availability data.

## 6. Confirmation for side effects
- Tools are annotated read-only vs. destructive (MCP tool annotations).
- Any action that commits the user (booking, cancelling, cart mutation, fare hold) requires an explicit confirmation step: without `confirm: true` the tool only returns a preview.
- Payment is never automated. Desktop connectors refuse to press any control whose label pays.

## 7. Sensitive data
- Health data (Doctolib) is never written to logs, cache or disk by autocruise.
- No telemetry.
