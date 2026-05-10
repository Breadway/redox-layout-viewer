# Redox Layout Viewer

Redox Layout Viewer is a small Rust application for watching a Redox keyboard's live Vial/VIA layout and rendering it as an on-screen keyboard. It reads the device over raw HID, decodes the keyboard definition and keymap, tracks layer changes, and can optionally export a live snapshot as JSON.

## Features

- Reads Vial/VIA data directly from the keyboard over raw HID.
- Renders the current keyboard layout with live key highlighting.
- Tracks layer changes from the device.
- Exports a compatible `current-layout.json` snapshot when requested.
- Includes a simple launcher script for release builds.

## Requirements

- Rust toolchain with `cargo`
- Linux
- A compatible Redox keyboard exposing the raw HID interface used by Vial/VIA

## Build

```bash
cargo build --release
```

## Run

The helper script builds the release binary if needed and then launches it:

```bash
./run-redox-layout.sh
```

Or run the binary directly:

```bash
cargo run -- --vid 0x4D44 --pid 0x5244
```

## Snapshot export

To write the current layout snapshot to disk:

```bash
cargo run -- --output current-layout.json
```

The snapshot is optional and is mainly there for compatibility with tools that expect a JSON export.

## Repository layout

- `src/main.rs` - Rust application entry point and renderer.
- `run-redox-layout.sh` - release launcher.
- `SCRIPTS.md` - implementation notes and design overview.
- `current-layout.json` - generated example snapshot.

## Notes

The code is intentionally self-contained. If you want to use a different keyboard, adjust the VID/PID arguments or extend the device discovery logic in `src/main.rs`.