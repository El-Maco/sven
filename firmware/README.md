# Firmware

Rust `no_std` firmware for the ESP32-S3. It owns the motor pins, serves the
embedded web UI, and exposes the HTTP API. There is no MQTT client.

## Build from the monorepo root

```bash
cp firmware/.env.example firmware/.env   # edit SSID, PASSWORD, STATIC_IP
just flash
```

The firmware embeds `../web/dist/index.html`, so the web build must run first.
`build.rs` fails with a clear message if it is missing. `just flash` handles
the order.

## Environment

| Variable | Default | Purpose |
|---|---|---|
| `SSID` | required | WiFi network name |
| `PASSWORD` | required | WiFi password |
| `STATIC_IP` | `192.168.1.10` | Desk IPv4 address |
| `PREFIX` | `24` | Subnet prefix length |
| `GATEWAY` | `192.168.1.1` | Default gateway |

`SSID` and `PASSWORD` use `env!` and must be set at compile time. The network
settings use `option_env!` with defaults.

## Toolchain

The crate uses the ESP Rust toolchain from `espup`:

```bash
cargo install espup --locked
espup install
```

`rust-toolchain.toml` selects the `esp` channel. The firmware also needs
`espflash` 3.x; `.cargo/config.toml` uses `espflash-3` as the runner.

## Structure

```
src/
  bin/main.rs        init, WiFi, static IP, task spawning
  net.rs             WiFi connection task, net task, address parsing
  desk/
    mod.rs           command channel and desk controller task
    gpio.rs          interruptible optocoupler pulse
    state.rs         motion model, state snapshot, calibration, buttons
  http/
    mod.rs           picoserve routes, SSE, embedded index.html
```

## Concurrency model

One task owns `SvenState` and the motor pins. HTTP handlers enqueue commands
through a `Channel` and read snapshots through a `Watch`. The desk task also
polls the physical buttons every 10 ms when no command is queued. `/api/stop`
sets an atomic flag that the pulse loop checks every 50 ms.

This means a long move cannot block the HTTP server, and there is exactly one
writer for the pin state.

## Dependency pin

`picoserve` is pinned to `0.17.1` because it supports `embassy-net 0.6` and
`embedded-io-async 0.6`, which match the working `esp-hal 0.23` / `esp-wifi
0.12` stack. `picoserve 0.20` requires `embassy-net 0.9`. Upgrade the whole
stack together in a separate change, not as part of a UI change.

## Memory

The firmware allocates an 80 KB heap and a 128 KB embassy task arena. Four web
tasks each hold 1 KB RX, 2 KB TX, and 2 KB HTTP buffers. `picoserve` uses no
heap for serving.

A 64 KB arena booted on the test ESP32-S3 but panicked with `embassy-executor:
task arena is full` when all four web tasks were spawned. 128 KB is the working
value. If RAM is tight, reduce `WEB_TASK_POOL_SIZE` to 3 before changing the
network stack.

## Endpoints

See [`../docs/api.md`](../docs/api.md).
