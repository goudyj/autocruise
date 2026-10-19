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
redroid ──(system HTTP proxy 172.30.0.2:1056)┐
Chrome  ──(SOCKS5 127.0.0.1:1055)────────────┤──▶ egress (tailscale) ──▶ exit node at home ──▶ Internet
autocruise itself, official APIs, the rest of the server ───────────────▶ server's own connection
```

Only the device and the browser go through home: if the home link is down,
they fail (`network`), and the rest of the server is unaffected.

Egress fixes the IP address only. It does not hide that the device is an
emulator; a service may still refuse it. Then try a real phone, or give up.

## Setup

1. **At home**, on an always-on machine (NAS, mini PC…) on your tailnet:
   enable IP forwarding, then advertise it as an exit node:

   ```bash
   echo -e 'net.ipv4.ip_forward = 1\nnet.ipv6.conf.all.forwarding = 1' | sudo tee /etc/sysctl.d/99-tailscale.conf
   sudo sysctl -p /etc/sysctl.d/99-tailscale.conf
   sudo tailscale set --advertise-exit-node
   ```

   Then approve it in the Tailscale admin console: machine → *Edit route
   settings* → *Use as exit node*.
2. **On the server**:

   ```bash
   AUTOCRUISE_EXIT_NODE=100.x.y.z \
     docker compose -f deploy/android/compose.yaml -f deploy/egress/compose.yaml up -d
   ```

   First run without \`TS_AUTHKEY\`: the container gives up and restarts if
   nobody logs in within about a minute, with a new URL each time. Log in once
   with a temporary container that waits indefinitely; the state is kept in
   the volume:

   ```bash
   docker run -d --name ts-login -v autocruise-egress-state:/var/lib/tailscale \\
     --cap-add NET_ADMIN --device /dev/net/tun tailscale/tailscale:stable \\
     tailscaled --statedir=/var/lib/tailscale --socket=/tmp/ts.sock
   docker exec -d ts-login tailscale --socket=/tmp/ts.sock up --hostname=autocruise-egress --timeout=0
   docker exec ts-login tailscale --socket=/tmp/ts.sock status   # prints the login URL
   docker rm -f ts-login                                          # after logging in
   ```

   Point the device at the egress proxy (kept across restarts in its data volume):

   ```bash
   adb -s 127.0.0.1:5555 shell settings put global http_proxy 172.30.0.2:1056
   ```

   The device does **not** share the egress network namespace: Android's netd
   installs policy routing that breaks tailscaled.

   \`AUTOCRUISE_EXIT_NODE\` must be the exit node's **Tailscale IP** (\`tailscale ip -4\`
   on that machine): a host name cannot be resolved while the container logs in.
   The container joins your tailnet as `autocruise-egress`; its state is kept
   in the `autocruise-egress-state` volume. Alternatively set `TS_AUTHKEY`.
3. **Check** the exit IP is your home's:

   ```bash
   docker exec autocruise-egress wget -qO- https://api.ipify.org  # egress itself
   curl -s --socks5-hostname 127.0.0.1:1055 https://api.ipify.org  # browser path
   ```

## Browser

Start the Chrome that autocruise attaches to with
`--proxy-server=socks5://127.0.0.1:1055`, in its own profile. Chrome resolves
host names through a SOCKS5 proxy, so DNS goes home too.

## Turning it off

`docker compose -f deploy/android/compose.yaml up -d` (without the egress file)
and \`adb shell settings put global http_proxy :0\` put the device back on the
server's connection.
