**Scripts Overview**

This document explains the purpose, structure, and usage of the Rust app in this workspace. It is intended to help the next developer quickly understand, run, and modify the code.

**Purpose**
- **Overview:** The project now runs as a single Rust binary that talks to the keyboard over raw HID, decodes Vial/VIA layout data, and renders the live layout with key highlighting.
- **One binary:** acquisition, parsing, snapshot export, and rendering now live in the same executable.

**Files**
- **Rust app:** [Cargo.toml](Cargo.toml), [src/main.rs](src/main.rs)
- **Launcher:** [run-redox-layout.sh](run-redox-layout.sh)
- **Generated snapshot:** [current-layout.json](current-layout.json)

**Rust app**
- **Role:** Open the keyboard, decode the active layout, render the board, and keep the view updated from live HID state.
- **Key responsibilities:**
  - HID client: reads the Vial identity, definition blob, VIA protocol info, layer count, layout options, and keymap buffer.
  - Layout iterator: keeps the KLE-compatible `iter_keyboxes()` semantics for row baselines, `rx/ry` rotation origins, and per-key `w/h` resets.
  - Render cache: prepares drawable boxes and labels so the paint path only iterates cached geometry.
  - Color and label mapping: `color_for_keybind()` and `pretty_bind_label()` control the visual language.
  - Optional snapshot export: `--output current-layout.json` writes the live state to disk for compatibility.

**How to run**
- Run the combined Rust app directly:

```bash
cargo run -- --vid 0x4D44 --pid 0x5244
```

- Optional compatibility output:

```bash
cargo run -- --output current-layout.json
```

- The snapshot file is optional now; the Rust binary owns both responsibilities internally.

**Customization & tuning**
- **Colors:** edit `color_for_keybind()` in [src/main.rs](src/main.rs). It controls the fill color used for each key.
- **Symbols & labels:** adjust `pretty_bind_label()` in [src/main.rs](src/main.rs) to change text/aliases.
- **Performance:** the Rust app already caches layout computation; a next step would be a text-layout cache keyed by `(label, font_size)`.
- **HID-based highlighting:** the current app highlights keys from focused UI input. To use device-reported presses instead, extend the background HID reader to emit pressed-key state and feed it into the render cache.

**Developer notes / quick pointers**
- The KLE parsing logic lives in `iter_keyboxes()` inside [src/main.rs](src/main.rs); it preserves row Y progression, rotation origin handling (`rx/ry`), and `w/h` inheritance resets.
- The renderer centers and scales using rotation-aware bounds; when adjusting padding/centering, look at the cache builder and `draw_box()` in [src/main.rs](src/main.rs).
- The optional JSON snapshot writer stays in sync with the live layer state when `--output` is set.

**Next steps / suggestions**
- Add a small GUI control to nudge alignment (x/y) interactively for users who want manual calibration.
- Optionally wire the HID device matrix into the UI so it can reflect true physical keypress state even when the window is not focused.
- Consider moving the HID protocol constants and keycode tables into a small module if you want to split the crate into reusable pieces later.

**Contact / Attribution**
- This file was generated to help future contributors quickly understand and modify the combined Rust app and the legacy Python references.
