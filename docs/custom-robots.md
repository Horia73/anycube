# Connect a custom cube robot

AnyCube can take six labelled photographs, read the stickers, validate the cube, solve it, and send your controller a logical sequence such as `R U2 F'`. Your controller handles presentation of the faces, mapping the cube frame to its own mechanism, and executing the sequence. AnyCube already ships the Solution Link adapter. To use it, implement the protocol below in your controller or in a local program that talks to the controller. There is no required board, motor layout, or host motion planner.

## Try it without hardware

From the checkout, start the normal local AnyCube app with the Python 3 emulator as its controller.

```sh
ANYCUBE_SOLUTION_LINK_CONTROLLER="$PWD/scripts/solution_controller.py" anycube app
```

If building from source, replace `anycube app` with `cargo run --release -- app`. Open **Robot**, select **Custom controller (Solution Link)**, choose **Connect**, then press **Scan & solve**. The emulator supplies six PNG images of its scrambled virtual 3×3×3. AnyCube reads those pixels through its regular vision pipeline. The emulator translates each received token to an actuator channel and signed quarter-turn count, applies those commands to its own sticker model, and prints `Fixture solved: True`. Disconnect and reconnect for a fresh scramble. To pass options, point the environment variable to an executable wrapper that launches the emulator with those arguments. `--no-half` demonstrates unsupported notation, `--reject` rejects execution, and `--delay 2` makes it easy to try Stop during movement.

For a controller using the app camera, advertise `app` instead of `png` and present each stable face to a camera selected in AnyCube. The emulator's `--camera app --frames-dir /tmp/robot-frames` writes synthetic frames for a test camera feed. Selecting this mode alone does not create a webcam.

If a proprietary controller already has a different wire protocol, build a local bridge exposing Solution Link or implement a Rust `Driver` and `Robot` under `src/robot/` and register it in `drivers()` in `src/robot/mod.rs`. The bridge needs no changes to AnyCube. A native driver needs a custom build and its own connection, protocol, and capability handling. It can use the normal solver with `Robot::controller_planned` and wrap the logical turns directly for execution, as the reference adapter does. MultiCuber 2.2 uses its existing firmware protocol and host-side planner. The Timer1 build of 2.3 negotiates controller-side planning.

## Solution Link v1

Set `ANYCUBE_SOLUTION_LINK_CONTROLLER` to the absolute path of a trusted local executable. It appears in the Robot picker. When you connect manually, AnyCube starts it with no arguments and sends commands to its stdin. Write protocol replies to stdout and diagnostics to stderr. Use a wrapper executable to configure a bridge that opens serial, USB or a network connection to your board. The adapter launches the configured path directly, without shell interpretation. The Python emulator uses Unix pipes and has been tested on Linux.

Records are printable ASCII terminated by LF with a maximum of 128 bytes before LF. The controller stays running through scan and solve. A reply of `ERR <message>` fails the operation. There are no automatic retries. In the transcript, `>` means AnyCube sends and `<` means the controller sends. The prefixes are omitted on the wire.

```text
> HELLO 1
< SOLUTION 1 app 3 1 1 350
> SCAN
< OK
< FACE 0 F 0
> ACK 0
```

`SOLUTION 1 <app|png> <size> <max-width> <half> <move-ms>` reports one loaded cube size, the maximum number of layers in a turn, half-turn support (0 or 1), and a per-move time estimate (0 for unknown, up to 60,000 ms). AnyCube uses its normal solver and displays the optional estimate. A uniform estimate does not change which solution it chooses. MultiCuber keeps its own time-based solution search.

### Scan frame and acknowledgement

Send six `FACE <index> <label> <clockwise-degrees>` records, with consecutive indices 0–5 and each label in `URFDLB` used exactly once, in any order. Labels refer to one fixed logical cube frame throughout the run. They are positions, independent of sticker colour. Each picture is seen looking straight at the outside of the labelled face, without mirroring. After rotating the raw pixels clockwise by 0, 90, 180 or 270 degrees, the rows and columns must match this table.

| Label | Top edge touches | Right edge touches |
| --- | --- | --- |
| U | B | R |
| R | U | B |
| F | U | R |
| D | F | R |
| L | U | F |
| B | U | L |

In `app` mode send `FACE` once the face is stable, then hold until `ACK <index>`. The browser captures a fresh frame. In `png` mode send `FACE`, one or more `DATA <base64-fragment>` records, then `END`. Concatenate fragments before decoding. Standard base64 with padding carries one PNG, with a total of at most 256 KiB of base64 (192 KiB decoded) and dimensions of at most 2048 pixels per axis. Use a centred, unobstructed face with a margin and a grid the detector can see. Start near 320×320 and crop or downscale on the controller if needed. The browser centre-crops to square and applies the requested rotation in both modes.

`ACK` means the app has the pixels, including successful decoding in PNG mode. The app can analyse them after the controller advances. Wait for the matching ACK even for the last frame, then send `DONE` when scanning and any repositioning are complete. An ACK is neither a vision result nor approval to manipulate stickers. You may reorient the cube as a whole, but preserve its sticker state and track the logical frame for the subsequent solve. If AnyCube cannot read or validate the cube, it sends no solution.

### Transfer and execute a solution

After a successful scan and validation, a solve can run once for that scan. For the example sequence, the following is the full solve exchange.

```text
> SOLVE 3
< OK
> MOVE R
< OK
> MOVE U2
< OK
> MOVE F'
< OK
> RUN
< ACCEPT
< MOVED 0
< MOVED 1
< MOVED 2
< DONE
```

`SOLVE <count>` starts an empty queue. Accept a `MOVE` only after validating it and reserving storage for it. `OK` confirms staging only. Do not move the cube during staging. Validate the complete batch before replying to `RUN` with `ACCEPT`, then start execution. Send one ordered, zero-based `MOVED <index>` when that logical turn has actually completed. `DONE` means every turn and any final mechanism motion have completed. AnyCube advances its displayed cube from completion reports and waits for `DONE` before showing success. An already solved cube sends no motion batch.

Moves are clockwise quarter turns of `URFDLB` viewed from outside the named face, with `'` for counterclockwise and `2` for half turns. On larger cubes `Rw` turns two adjacent layers from the R face and `3Rw` turns three. Suffixes also apply to wide moves. V1 carries no slices or whole-cube rotations. The handshake permits sizes 2–50 within the app's solver limit and widths from 1 to size minus 1. AnyCube checks the complete normal solution against the advertised width and half-turn support before it sends `SOLVE`. It reports an unsupported move if the sequence exceeds those capabilities. It does not search for a restricted-notation alternative. The emulator and session regression cover 3×3×3. Wider tokens are covered at the adapter level. Production use on other sizes depends on your controller, camera setup and available AnyCube solver tables.

Every host command is short and staging is stop-and-wait, so the host never floods a receiver with a long algorithm line. The wire cap is 4,096 moves. A constrained controller should enforce its own smaller batch limit at `SOLVE` and may store compact parsed moves. For a Mega using a local serial bridge, preserve backpressure and forward STOP and link loss promptly. Stream image chunks from the camera or bridge instead of allocating a whole PNG in a small MCU.

### Stop and connection loss

Handle `STOP` and connection loss during staging, scan, image transfer, and motion. Halt safely, discard staged and remaining moves, and require a new connection and scan. Your controller owns actuator shutdown, local watchdogs and any physical emergency stop. AnyCube sends `STOP` best effort, then closes a stopped or failed link. No response to STOP is required, and the app cannot confirm a physical stop. An idle disconnect signals EOF without asking the robot to park. The adapter gives the process two seconds to exit before terminating it. No moves are replayed and the adapter sends no follow-up control on that connection. Reconnect manually after a stop or wire failure.

AnyCube waits up to 3 seconds for immediate replies and for the browser's pixel acknowledgement, and up to 60 seconds per face, per complete image transfer, per move completion, and for final completion. Design the controller to fail safely if its own wait for the host expires. A bridge should abort and exit if it loses its hardware connection. Use a controller-local watchdog if a stalled controller or a broken bridge could leave hardware moving.

## Run the integration regression

On Linux with Python 3, Node.js 22+, Rust, and AnyCube's 3×3×3 tables, run `cargo test --test solution_link -- --ignored`. It uses the normal robot session, the app's face detector and colour assembly, and the independent controller process. It exercises PNGs and app-camera acknowledgements with synthetic pixels, size and notation rejection, controller rejection, and stopping mid-run.

### Direct USB

The MultiCuber picker negotiates the firmware version when it connects. To pin a port explicitly, set `ANYCUBE_SOLUTION_LINK_SERIAL` to the selected serial device and select
its Solution Link entry. This takes precedence over the subprocess adapter.
With this setting discovery does not probe devices. The port opens at 115200
8N1. MultiCuber identification uses the legacy HELLO first. Version 2.2
uses the legacy driver. Version 2.3 receives HELLO 1 and independent KEEP
records every 200 ms. During a face hold, an explicitly failed capture
requests RESTORE and waits for OK and DONE. A silent capture timeout, Stop and active disconnect send
STOP. Idle disconnect ends the lease with BYE. A physically unplugged Mega
can only detect host loss when its firmware lease expires.

For a serial controller implementing Solution Link directly, also set
`ANYCUBE_SOLUTION_LINK_PROTOCOL=native`. Its first command is HELLO 1.
Alongside app and png, a controller may advertise state as its source.
Answer SCAN with OK, one or more `STATE <facelets>` records in URFDLB
order, then DONE. The concatenated state must contain exactly 6×N×N
stickers and passes through the solver's state validation before RUN.
The app requests its camera only when starting a camera scan or when the
user explicitly starts the camera.
