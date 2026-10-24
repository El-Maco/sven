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
| `position` | `Bottom`, `Top`, `Armrest`, `AboveArmrest`, `Standing`, or `Custom`. |
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
| `UpRelative` | millimetres | Requires calibration. |
| `DownRelative` | millimetres | Requires calibration. |
| `AbsoluteHeight` | millimetres | Clamped to 622..1274. Requires calibration. |
| `Position` | position index | `0` Bottom, `1` Top, `2` Armrest, `3` AboveArmrest, `4` Standing, `5` Custom. |
| `Calibrate` | position index | Homes to the nearest end, then moves to the position and marks the desk calibrated. |

Position indexes match the `SvenPosition` enum order. The firmware keeps this
schema for compatibility with the existing UI. A future version may accept a
position name instead of an index.

Responses:

- `202 Accepted` with `{"accepted":true}` when the command is queued.
- `409 Conflict` with `{"accepted":false}` when the queue is full. Retry after
  the current move finishes.
- `400 Bad Request` when the JSON is malformed. The firmware never panics on a
  bad request.

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
  -d '{"command":"Calibrate","value":4}'

curl -X POST http://192.168.1.10/api/stop

curl -N http://192.168.1.10/api/events
```

## Calibration behavior

The desk has no encoder. `height_mm` is an estimate from the duration table.
After a reboot `calibrated` is false and `height_mm` is `0`. Relative,
absolute, and position moves are rejected by the UI until calibration. Duration
moves remain available because they do not depend on the estimate.

Calibration chooses the nearest end based on the current estimate, runs the
motor until it reaches that end, then moves to the requested position. If the
estimate is unknown, it homes to the bottom.
