# Sven

Sven is a standing desk controller. The ESP32-S3 hosts the web UI and the
JSON API directly, at a static IP on the local network.

```
phone / laptop  --->  ESP32-S3 192.168.1.10:80
                        |- /              single-file web UI
                        |- /api/state     current desk state
                        |- /api/events    server-sent events
                        |- /api/command   queue a movement
                        |- /api/positions saved height presets
                        '- /api/stop      stop immediately

MQTT broker  <---->  ESP32-S3 (retained sven/state, sven/positions)
```

The firmware keeps no filesystem. It restores the last desk estimate and the
position presets from retained MQTT messages, and publishes changes back to the
broker. The UI is built to one self-contained `index.html` and embedded in the
firmware image.

## Layout

| Path | Purpose |
|---|---|
| `firmware/` | Rust `no_std` ESP32-S3 firmware, `esp-hal` + `embassy` + `picoserve` + MQTT persistence |
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

Set `MQTT_HOST` to the broker's LAN IP and `MQTT_PORT` (default `1883`).
`MQTT_USERNAME` and `MQTT_PASSWORD` are optional. Use an IP address rather than
a hostname because the firmware does not configure a DNS server. If
`MQTT_HOST` is empty, the firmware runs without persistence.

The firmware build reads `firmware/.env` automatically through `build.rs`, so
you do not need to export these variables. Values already exported in your
shell take precedence over the file. Rebuild after editing `.env`; Cargo
tracks the file and picks up the change.

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
table, interpolated between the measured points and extrapolated at the
measured steady-state speed. Relative moves are sent as one continuous pulse
for the whole distance instead of being stepped through the table, so the
motor does not stop and restart mid-move. User-defined positions are loaded
from the retained `sven/positions` topic, and the last height/calibration
state is restored from `sven/state` on boot. On a fresh broker the firmware
seeds the three original presets once.

Calibration homes to the nearest end of the desk, drives briefly past that end
so it settles against the physical stop, then moves to the selected position.
The extra travel clears the error accumulated by open-loop moves. Before the
first calibration the estimate is unknown, so it homes to the bottom over the
full travel range.

Retained state lives on the broker. Enable broker persistence if the state
must also survive a broker restart. If the desk is moved while the ESP32 is
powered off, the restored estimate is stale; only an encoder could detect
that.

Duration moves are always available. Relative, absolute, and position moves
require calibration.

## Remote access

See [`docs/tailscale.md`](docs/tailscale.md). The recommended setup is a
Tailscale subnet router on an always-on LAN host. The ESP32 stays a plain HTTP
device and does not run Tailscale itself.

## API

See [`docs/api.md`](docs/api.md) and [`docs/hardware.md`](docs/hardware.md).
