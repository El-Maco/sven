# Hardware

The controller is an ESP32-S3 driving the desk's existing manual-control
inputs through optocouplers. It does not replace the desk controller; it
simulates button presses.

The test board reports:

- Chip: ESP32-S3, revision v0.2
- Flash: 16 MB
- MAC: `74:4d:bd:7d:4a:1c`
- USB: native USB JTAG/serial, `/dev/ttyACM0`

## Pinout

| Signal | ESP32-S3 pin | Direction | Notes |
|---|---|---|---|
| Desk up | `GPIO5` | Output | Active high, through optocoupler |
| Desk down | `GPIO7` | Output | Active high, through optocoupler |
| Physical up button | `GPIO9` | Input | Pull-down, active high |
| Physical down button | `GPIO10` | Input | Pull-down, active high |

The firmware drives the optocoupler inputs high for the requested duration,
then returns them low. The optocouplers isolate the ESP32 from the desk's
control electronics.

The schematic is at
[`firmware/docs/Schematic_Sven_2025-06-04.png`](../firmware/docs/Schematic_Sven_2025-06-04.png).
Component datasheets are under `firmware/docs/components/`.

## Power and boot behavior

The desk has no height encoder. The firmware estimates height from a
duration-to-distance table and keeps the estimate in RAM. On boot the estimate
is unknown until calibration.

If the ESP32 reboots while the desk is moving, the optocoupler outputs return
low when the GPIOs are initialized, so the desk stops. The desk controller's
own limits still apply.

## Wiring checks

- Confirm the optocoupler orientation and common ground before flashing.
- Confirm the ESP32 is powered from a stable 5 V source and that the 3.3 V
  logic levels are correct for the optocoupler input side.
- Keep the motor wiring away from the WiFi antenna and the USB data lines.
- The firmware sets the up/down pins low at boot before enabling the outputs.
