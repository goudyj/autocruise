# Using autocruise with OpenClaw

How to run autocruise as an MCP server for an [OpenClaw](https://docs.openclaw.ai)
Gateway, so its tools work from the Control UI and from chat channels
(Telegram…). Tested with OpenClaw 2026.9 on Ubuntu (X11, systemd).

## 1. Install the binary

```sh
cargo install --path crates/autocruise --locked     # from a clone; or:
cargo install --git https://github.com/goudyj/autocruise autocruise
```

Reinstall after every update: the Gateway runs the installed binary, not the
checkout.

## 2. Browser for SNCF Connect (only if you enable `sncf-connect`)

The Gateway's own browser tool drives Chrome over CDP, which SNCF Connect
blocks, and the Gateway's managed desktop (`:99`, the Control UI "This
machine" view) stops when nobody watches it, which closes Chrome. autocruise
therefore uses **its own persistent display** with your normal Chrome:

```sh
deploy/desktop/browser-desktop.sh install-service --vnc   # systemd user unit, restarts Chrome if it dies
sudo loginctl enable-linger "$USER"                        # keep it running without a login session
```

`--vnc` needs a password file first: `tigervncpasswd ~/.config/autocruise/vncpasswd`.
Then log in to sncf-connect.com **yourself**, once, through VNC
(`ssh -N -L 5950:127.0.0.1:5950 <host>`, then a VNC viewer on
`localhost:5950`). Without `--vnc` the display is invisible: fine while
everything works, but you need a way to see it when SNCF Connect asks for a
check or logs you out. Details: [connectors/sncf-connect.md](connectors/sncf-connect.md).

## 3. Register the MCP server

```sh
openclaw mcp add autocruise \
  --command "$HOME/.cargo/bin/autocruise" \
  --arg mcp --arg --enable --arg sncf,sncf-connect \
  --env AUTOCRUISE_DESKTOP_DISPLAY=:50 \
  --env AUTOCRUISE_DESKTOP_DBUS=unix:path=/run/user/$(id -u)/autocruise-desktop-50/bus \
  --timeout 300 --connect-timeout 20
openclaw mcp doctor autocruise --probe
```

- `--timeout 300` matters: a whole-day SNCF Connect search takes 1.5–2
  minutes, longer than the default 60 s request timeout.
- `--enable` lists the connectors (`sncf`, `sncf-connect`). Leave out
  `sncf` if you have no SNCF API token.
- The tools appear on the next message; no Gateway restart is needed.

### SNCF API token (`sncf` connector)

The official-API connector needs `SNCF_API_TOKEN` in the **server's**
environment. In OpenClaw 2026.9, `mcp.servers.*.env` only takes plain
strings (no SecretRef), and the Gateway's exec egress proxy does not apply
(autocruise calls `api.sncf.com` itself). So keep the token out of
`openclaw.json` and start the server through `deploy/autocruise-env.sh`,
which loads a private env file:

```sh
deploy/autocruise-env.sh --set SNCF_API_TOKEN   # hidden prompt, file mode 600
```

and use it as the command in step 3:
`--command "$PWD/deploy/autocruise-env.sh"`. The launcher refuses an env file
readable by others. The token still sits in plain text in that file (mode
600): that is the trade-off.

## 4. Agent skill (optional)

The repository ships `.agents/skills/autocruise/SKILL.md`: when to use which tool, how to present prices, how to hold a fare safely, what to do on each error, and a repair checklist. Copy or link it into your agent's skills directory (for OpenClaw, the workspace `skills/` folder) so the assistant follows it.

## 5. Approvals

`sncf_connect_hold_fare` is annotated destructive and only acts with
`confirm: true`; its description tells the model to show the preview and
ask you first. For a second safety net, make OpenClaw prompt before MCP tool
calls of this server (see "Approvals" in OpenClaw's MCP documentation).

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `user_action_required` "security check or block page" | SNCF Connect shows a DataDome check. Open the display (VNC) and complete or wait it out yourself, then ask again. autocruise never touches it. |
| `user_action_required` "not logged in" | Log in again in that Chrome. |
| `user_action_required` "no application matching chrome" / "cannot reach the desktop" | The display or Chrome is not running: `systemctl --user status autocruise-desktop.service`. |
| `rate_limited` "another autocruise process is using the browser" | Two searches at once (e.g. a CLI call while the Gateway runs one). It waits up to 3 minutes, then gives up: retry. |
| Tool call times out after 60 s | `--timeout` not set on the MCP server (step 3). |
| Tools missing after an update | Reinstall the binary (step 1); `openclaw mcp doctor autocruise --probe`. |
| Stray `IDENTITY.md`, `SOUL.md`, `USER.md`, `memory/` in the checkout | OpenClaw seeded workspace templates because an agent session used the checkout as its workspace. Not part of autocruise: delete them, and start agent sessions from the OpenClaw workspace instead. |
