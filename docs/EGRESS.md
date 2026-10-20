# Egress through the user's home connection (optional)

> **You do not need this** if autocruise runs at home: the browser already uses
> your connection. Nothing in autocruise depends on it; it is a separate Docker
> Compose file you add only if you run autocruise on a server.

autocruise is meant to run at home. If you run it on a server (VM, VPS), the
sites it drives through the browser see a **datacenter IP**, which anti-bot
systems often block outright. The optional egress container sends the
browser's traffic out through **your own home connection** instead, using a
Tailscale exit node you run at home.

This is your real network, used by you, for your own accounts. Third-party,
residential or rotating proxies stay forbidden ([PRINCIPLES.md](PRINCIPLES.md), rule 4).

```
Chrome ──(SOCKS5 127.0.0.1:1055)──▶ egress (tailscale) ──▶ exit node at home ──▶ Internet
autocruise itself, official APIs, the rest of the server ──▶ server's own connection
```

Only the browser goes through home: if the home link is down, it fails
(`network`), and the rest of the server is unaffected.

## Setup

1. **At home**, on an always-on machine on your tailnet, enable IP forwarding
   (and, if Docker or ufw runs there, allow forwarding from `tailscale0`), then
   advertise it as an exit node:

   ```bash
   echo -e 'net.ipv4.ip_forward = 1\nnet.ipv6.conf.all.forwarding = 1' | sudo tee /etc/sysctl.d/99-tailscale.conf
   sudo sysctl -p /etc/sysctl.d/99-tailscale.conf      # writing the file alone is not enough
   sudo tailscale set --advertise-exit-node
   ```

   Then approve it in the Tailscale admin console: machine → *Edit route
   settings* → *Use as exit node*.

2. **On the server**, load the netfilter modules the container needs (recent
   kernels; once per boot, or list them in `/etc/modules-load.d/`):

   ```bash
   sudo modprobe -a nf_tables nft_ct nft_chain_nat nft_masq nft_compat xt_mark xt_connmark ip_tables iptable_filter iptable_nat iptable_mangle
   ```

   First run without `TS_AUTHKEY`: the container gives up and restarts if
   nobody logs in within about a minute, with a new URL each time. Log in once
   with a temporary container that waits indefinitely; the state is kept in
   the volume:

   ```bash
   docker run -d --name ts-login -v autocruise-egress-state:/var/lib/tailscale \
     --cap-add NET_ADMIN --device /dev/net/tun tailscale/tailscale:stable \
     tailscaled --statedir=/var/lib/tailscale --socket=/tmp/ts.sock
   docker exec -d ts-login tailscale --socket=/tmp/ts.sock up --hostname=autocruise-egress --timeout=0
   docker exec ts-login tailscale --socket=/tmp/ts.sock status   # prints the login URL
   docker rm -f ts-login                                          # after logging in
   ```

   Then start it. `AUTOCRUISE_EXIT_NODE` must be the exit node's **Tailscale
   IP** (`tailscale ip -4` on that machine): a host name cannot be resolved
   while the container starts.

   ```bash
   AUTOCRUISE_EXIT_NODE=100.x.y.z docker compose -f deploy/egress/compose.yaml up -d
   ```

3. **Check** the exit IP is your home's:

   ```bash
   curl -s --socks5-hostname 127.0.0.1:1055 https://api.ipify.org
   ```

## Browser

Start the Chrome that autocruise attaches to with
`--proxy-server=socks5://127.0.0.1:1055`, in its own profile. Chrome resolves
host names through a SOCKS5 proxy, so DNS goes home too.

## Turning it off

`docker compose -f deploy/egress/compose.yaml down`, and start Chrome without
the proxy flag.
