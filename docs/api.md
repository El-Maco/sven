# Sven HTTP API

Base URL: `http://192.168.1.10` (the static IP configured in the firmware).

All responses are JSON unless noted. The API is same-origin with the web UI,
so no CORS preflight is needed. There is no authentication on the LAN. Do not
expose port 80 directly to the internet; use Tailscale or a reverse proxy with
its own access control.

## `GET /`

Returns the embedded single-file web UI as `text/html; charset=utf-8` with
`Cache-Control: no-cache`.

## `GET /api/state`

Returns a snapshot of the desk.

```json
{
  "height_mm": 812,
  "position": "Custom",
  "moving": true,
  "target_mm": 900,
  "calibrated": true
}
```

| Field | Meaning |
|---|---|
| `height_mm` | Estimated height. `0` means unknown when `calibrated` is false. |
| `position` | `Bottom`, `Top`, a saved position name, or `Custom`. |
| `moving` | The desk controller is executing a command. |
| `target_mm` | Target height for the current command, or `null`. |
| `calibrated` | The estimate is anchored to a known end of the desk. |

## `GET /api/events`

Server-Sent Events stream. The server sends one event when a client connects
and then on every state change. A keepalive is sent every 15 seconds.

```
event: state
data: {"height_mm":812,"position":"Custom","moving":true,"target_mm":900,"calibrated":true}
```

Browsers reconnect automatically. The UI also falls back to polling
`/api/state` once per second if the stream cannot be established.

## `POST /api/command`

Queues a movement. Returns `202 Accepted` immediately. The command is executed
by the single desk-controller task.

```json
{ "command": "AbsoluteHeight", "value": 800 }
```

Commands and value units:

| Command | `value` | Notes |
|---|---|---|
| `UpDuration` | milliseconds | Runs up for the given time. Always allowed. |
| `DownDuration` | milliseconds | Runs down for the given time. Always allowed. |
| `UpRelative` | millimetres | Requires calibration. Clamped to the remaining travel and sent as one pulse. |
| `DownRelative` | millimetres | Requires calibration. Clamped to the remaining travel and sent as one pulse. |
| `AbsoluteHeight` | millimetres | Clamped to 622..1274. Requires calibration. Sent as one estimated pulse. |
| `Position` | position index | `0` Bottom, `1` Top, `2+` saved positions in `/api/positions` order. |
| `Calibrate` | position index | Homes past the nearest end to clear accumulated error, then moves to the position and marks the desk calibrated. |

`Bottom` and `Top` are built-in calibration ends. Saved positions start at
index `2`. Adding or removing a position shifts the indexes of later entries,
so fetch `/api/positions` before sending a position command.

Responses:

- `202 Accepted` with `{"accepted":true}` when the command is queued.
- `409 Conflict` with `{"accepted":false}` when the queue is full. Retry after
  the current move finishes.
- `400 Bad Request` when the JSON is malformed. The firmware never panics on a
  bad request.

## `GET /api/positions`

Returns the saved positions in the order used by the `Position` command.

```json
[
  { "name": "Armrest", "height_mm": 750 },
  { "name": "Standing", "height_mm": 1140 }
]
```

## `POST /api/positions`

Adds a position or updates the height of an existing name.

```json
{ "name": "Reading", "height_mm": 900 }
```

Returns `200 OK` with `{"accepted":true}`, or `400 Bad Request` with
`{"accepted":false}` when the name is reserved (`Bottom`, `Top`, `Custom`),
too long, contains unsupported characters, the height is outside `622..1274`,
or the eight-position limit is reached.

## `POST /api/positions/remove`

```json
{ "name": "Reading" }
```

Returns `200 OK` with `{"accepted":true}`, or `400 Bad Request` when the name
does not exist.

Position changes are published to the retained `sven/positions` topic.

## `POST /api/stop`

Sets the stop flag. The next 50 ms motion chunk stops the motor and the
command returns. Returns `202 Accepted` with `{"accepted":true}`.

The stop flag is cleared automatically when the move ends or when the next
command starts.

## `GET /api/info`

```json
{
  "name": "sven",
  "version": "0.2.0",
  "ip": "192.168.1.10"
}
```

## Examples

```bash
curl http://192.168.1.10/api/state

curl -X POST http://192.168.1.10/api/command \
  -H 'Content-Type: application/json' \
  -d '{"command":"UpDuration","value":2000}'

curl -X POST http://192.168.1.10/api/command \
  -H 'Content-Type: application/json' \
  -d '{"command":"Calibrate","value":2}'

curl -X POST http://192.168.1.10/api/stop

curl -N http://192.168.1.10/api/events
```

## Motion model

The desk is open loop: there is no encoder and no limit-switch input. The
firmware maps motor on-time to travel with the measured table in
`firmware/src/desk/motion.rs`. Values between measured points are linearly
interpolated, and travel beyond the last point is extrapolated at the measured
steady-state speed of 38 mm/s.

Relative, absolute, and saved-position moves are sent as a single continuous
motor pulse. The whole distance is converted to a duration up front, so the
motor does not stop and restart (and lose its ramp) partway through the move.
Bottom and Top add an end-stop guard; other positions use the same distance
model as absolute moves.

Relative moves are clamped to the calibrated range, so an oversized request
stops at the nearest end instead of running past it.

## MQTT persistence

When `MQTT_HOST` is configured, the firmware uses two retained topics:

| Topic | Payload |
|---|---|
| `sven/state` | The JSON snapshot returned by `/api/state` |
| `sven/positions` | The JSON array returned by `/api/positions` |

The firmware subscribes to both on boot and restores `height_mm` and
`calibrated` from `sven/state`. `moving` and `target_mm` are reset, and the
current position name is recomputed from the loaded positions. If
`sven/positions` is missing, the three original presets are published once.

Retain makes the broker hold the last message. Enable broker disk persistence
if the state must also survive a broker restart. If the desk is moved while the
ESP32 is off, the restored height is stale.

## Calibration behavior

The desk has no encoder. `height_mm` is an estimate from the duration table.
After a reboot the firmware restores the last calibrated height from MQTT. If
the broker is unavailable or has no retained state, `calibrated` is false and
`height_mm` is `0`. Relative, absolute, and position moves are rejected by the
UI until calibration. Duration moves remain available because they do not
depend on the estimate.

Calibration chooses the nearest end based on the current estimate, runs the
motor past that end for a short guard window so the desk settles against the
physical stop, then moves to the requested position. The guard clears error
accumulated by open-loop moves. If the estimate is unknown, it homes to the
bottom over the full travel range.
