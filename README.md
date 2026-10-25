# Sven

Sven is a standing desk controller. The ESP32-S3 hosts the web UI and the
JSON API directly, at a static IP on the local network.

```
phone / laptop  --->  ESP32-S3 192.168.1.10:80
                        |- /              single-file web UI
                        |- /api/state     current desk state
                        |- /api/events    server-sent events
                        |- /api/command   queue a movement
                        '- /api/stop      stop immediately
```

There is no MQTT broker, no API bridge, and no separate web server. The UI is
built to one self-contained `index.html` and embedded in the firmware image.

## Layout

| Path | Purpose |
|---|---|
| `firmware/` | Rust `no_std` ESP32-S3 firmware, `esp-hal` + `embassy` + `picoserve` |
| `web/` | Vite + React + Tailwind UI, built to a single file |
| `docs/` | API, Tailscale, and hardware notes |
| `legacy/` | Archived backend/frontend from the MQTT-era architecture |

## Prerequisites

- Rust ESP toolchain installed with [`espup`](https://github.com/esp-rs/espup):
  ```bash
  cargo install espup --locked
  espup install
  . "$HOME/export-esp.sh"
  ```
- `espflash` 3.x, available as `espflash-3`:
  ```bash
  cargo install espflash --version 3.3.0 --bin espflash --root "$CARGO_HOME/espflash-3.3.0"
  ln -s "$CARGO_HOME/espflash-3.3.0/bin/espflash" "$CARGO_HOME/bin/espflash-3"
  ```
- The `just` recipes source `$HOME/export-esp.sh` automatically if it exists.
- Serial access to the ESP32. The USB port is usually `/dev/ttyACM0` and is
  owned by `root:uucp`. Add your user to `uucp`
  (`sudo usermod -aG uucp $USER`, then log out and back in), or run
  `sudo chown $USER:$USER /dev/ttyACM0` after plugging the board in. The
  helper script does the second option:
  ```bash
  ./firmware/modprobe.sh
  ```
  Run it as your user, not with `sudo`, so the script can preserve `$USER`.
- Node.js 20+ and `pnpm`.
- `just` (optional, but the commands below assume it).

## Configuration

```bash
cp firmware/.env.example firmware/.env
```

Set `SSID`, `PASSWORD`, `STATIC_IP`, `PREFIX`, and `GATEWAY`. The default
address is `192.168.1.10/24` with gateway `192.168.1.1`. Pick an address
outside the router's DHCP pool.

## Build and flash

```bash
just flash
```

This builds the web UI, builds the firmware, and runs it through `espflash`.
Connect the ESP32-S3 over USB first.

The firmware uses a 3 MB factory app partition (`firmware/partitions.csv`) so
the embedded UI fits comfortably. The `espflash` runner is configured to flash
that table. This assumes at least 4 MB of flash, which the ESP32-S3 devkit
modules have.

Other commands:

```bash
just web        # build web/dist/index.html only
just dev-web    # Vite hot reload, proxying /api to 192.168.1.10
just check      # cargo check the firmware
just typecheck  # TypeScript check
```

Open `http://192.168.1.10` from a phone or laptop on the same network.

## Install on your phone

The ESP32 serves the icon, favicon, and web app manifest directly.

- **iPhone/iPad:** open `http://192.168.1.10/` in Safari, tap Share, then
  **Add to Home Screen**. iOS uses `icon-180.png`.
- **Android:** open `http://192.168.1.10/` in Chrome, open the menu, then
  **Add to Home screen** or **Install app**. The manifest provides the
  192 px and 512 px icons.

The icon is the supplied desk glyph, vectorized so it stays crisp at small
sizes. The original is kept at `web/public/icon-source.jpg`; the traced source
is `web/public/icon.svg`. The exports are:

- `icon-512.png` for app icons
- `icon-192.png` and `icon-180.png` for Android and iOS
- `icon-maskable-512.png` for Android's circular mask, with extra padding
- `icon-rounded-512.png` for general use with transparent rounded corners
- `favicon.svg` and `favicon-32.png` for the browser tab

## Calibration

The desk has no encoder. Height is estimated from the open-loop duration
table, and state is not persisted across reboots yet. On boot the UI shows
"Position unknown" until you enable calibration mode and choose a position.
Calibration homes to the nearest end of the desk, then moves to the selected
position.

Duration moves are always available. Relative, absolute, and position moves
require calibration.

## Remote access

See [`docs/tailscale.md`](docs/tailscale.md). The recommended setup is a
Tailscale subnet router on an always-on LAN host. The ESP32 stays a plain HTTP
device and does not run Tailscale itself.

## API

See [`docs/api.md`](docs/api.md) and [`docs/hardware.md`](docs/hardware.md).
