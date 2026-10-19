# Egress through the user's home connection (optional)

> **You do not need this** if autocruise runs at home: the device and browser
> already use your connection. Nothing in autocruise depends on it; it is a
> separate Docker Compose overlay you add only if you run autocruise on a server.

autocruise is meant to run at home. If you run it on a server (VM, VPS), the
sites and apps it drives see a **datacenter IP**, which anti-bot systems often
block outright. The optional egress container sends the browser and Android
device traffic out through **your own home connection** instead, using a
Tailscale exit node you run at home.

This is your real network, used by you, for your own accounts: the same
situation as using the app on your phone at home. It is not a proxy service.
Third-party, residential or rotating proxies stay forbidden
([PRINCIPLES.md](PRINCIPLES.md), rule 4).

```
redroid ──(shares egress network namespace)──┐
Chrome  ──(SOCKS5 127.0.0.1:1055)────────────┤──▶ egress (tailscale) ──▶ exit node at home ──▶ Internet
autocruise itself, official APIs, the rest of the server ───────────────▶ server's own connection
```

Only the device and the browser go through home: if the home link is down,
they fail (`network`), and the rest of the server is unaffected.

Egress fixes the IP address only. It does not hide that the device is an
emulator; a service may still refuse it. Then try a real phone, or give up.

## Setup

1. **At home**, on an always-on machine (NAS, mini PC…) on your tailnet:
   `sudo tailscale set --advertise-exit-node`, then in the Tailscale admin
   console approve it: machine → *Edit route settings* → *Use as exit node*.
2. **On the server**:

   ```bash
   AUTOCRUISE_EXIT_NODE=<home-machine> \
     docker compose -f deploy/android/compose.yaml -f deploy/egress/compose.yaml up -d
   docker logs autocruise-egress        # first run: open the login URL once
   ```

   The container joins your tailnet as `autocruise-egress`; its state is kept
   in the `autocruise-egress-state` volume. Alternatively set `TS_AUTHKEY`.
3. **Check** the exit IP is your home's:

   ```bash
   adb -s 127.0.0.1:5555 shell curl -s https://api.ipify.org     # device
   curl -s --socks5-hostname 127.0.0.1:1055 https://api.ipify.org  # browser path
   ```

## Browser

Start the Chrome that autocruise attaches to with
`--proxy-server=socks5://127.0.0.1:1055`, in its own profile. Chrome resolves
host names through a SOCKS5 proxy, so DNS goes home too.

## Turning it off

`docker compose -f deploy/android/compose.yaml up -d` (without the egress file)
puts the device back on the server's connection.
