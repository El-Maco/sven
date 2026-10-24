# Tailscale access

The ESP32 does not run Tailscale. It stays a plain HTTP device on the LAN, and
an always-on host acts as the tailnet gateway. This keeps the firmware small
and avoids replacing the Rust `no_std` network stack with ESP-IDF/lwIP.

## Recommended: subnet router on maco-server

`maco-server` is on the LAN at `192.168.1.142` and is currently online in the
tailnet. `minimaco` has been offline for a long time and should not be the
dependency.

On `maco-server`:

```bash
echo 'net.ipv4.ip_forward=1' | sudo tee /etc/sysctl.d/99-tailscale.conf
sudo sysctl -p /etc/sysctl.d/99-tailscale.conf
sudo tailscale set --advertise-routes=192.168.1.10/32
```

Then approve the route in the Tailscale admin console under the machine's
**Edit route settings**. Advertising a `/32` exposes only the desk. If your
Tailscale version rejects a `/32`, advertise `192.168.1.0/24` and use ACLs to
restrict access to the desk.

Use `tailscale set` rather than `tailscale up`. `up` expects the complete
desired configuration and can reset other settings; `set` changes only the
flag you pass.

On clients that need the route, enable subnet routes:

- Linux: `sudo tailscale set --accept-routes`
- Windows: `tailscale set --accept-routes`
- iOS/Android/macOS: enable "Use Tailscale subnets" in the Tailscale app.

Then open `http://192.168.1.10` from anywhere on the tailnet.

The gateway host must stay powered on. If it sleeps, remote access stops until
it returns.

## ACL example

Restrict tailnet members to the desk's HTTP port:

```json
{
  "acls": [
    {
      "action": "accept",
      "src": ["autogroup:member"],
      "dst": ["192.168.1.10:80"]
    }
  ]
}
```

Only tailnet members should be able to reach the desk. A desk is a physical
actuator, so treat control as sensitive.

## Optional: a stable HTTPS name

If you want `https://maco-server.tail9add64.ts.net` instead of an IP, run a
small reverse proxy on `maco-server` and expose it with Tailscale Serve.

Caddyfile:

```caddyfile
:8080 {
    reverse_proxy 192.168.1.10:80
}
```

Then:

```bash
sudo tailscale serve --bg --https=8443 http://localhost:8080
```

This terminates TLS at the gateway. The ESP32 still speaks plain HTTP on the
LAN. If you use a path prefix instead of a dedicated port, set the matching
Vite `base` and make the frontend API paths relative.

## Why not Tailscale on the ESP32

Embedded implementations exist (MicroLink, `tailscale-iot`). They are ESP-IDF
projects built on lwIP, mbedTLS, and WireGuard. Sven uses Rust `no_std` with
`esp-hal` and `embassy-net`. Integrating those implementations would mean
replacing the network stack or maintaining a second firmware, plus
control-plane registration, DERP relays, and key management on a device whose
job is to pulse two optocouplers.

A subnet router on an existing always-on host gives the same user-visible
result with no firmware risk. Revisit direct Tailscale only if the desk must be
reachable without any always-on LAN host. A travel router running Tailscale is
still simpler than changing the firmware.
