use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Sender};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::keycode::keycode_name;
use crate::types::{LayerState, Snapshot};

/// Spawns a background thread that serializes and writes snapshots so the
/// render thread never blocks on JSON encoding or disk I/O. Bursts are
/// coalesced: only the most recently queued state is written.
pub fn spawn_snapshot_writer(path: PathBuf, snapshot: Arc<Snapshot>) -> Sender<(LayerState, usize)> {
    let (tx, rx) = unbounded::<(LayerState, usize)>();
    thread::Builder::new()
        .name("snapshot-writer".into())
        .spawn(move || {
            while let Ok(mut latest) = rx.recv() {
                while let Ok(next) = rx.try_recv() {
                    latest = next;
                }
                let (state, layer) = latest;
                let _ = write_snapshot(&path, &snapshot, &state, layer);
            }
        })
        .expect("failed to spawn snapshot writer thread");
    tx
}

pub fn write_snapshot(
    path: &PathBuf,
    snapshot: &Snapshot,
    state: &LayerState,
    current_layer: usize,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let serialized =
        serde_json::to_string_pretty(&snapshot_to_json(snapshot, state, current_layer))?;
    fs::write(path, serialized + "\n")?;
    Ok(())
}

pub fn snapshot_to_json(
    snapshot: &Snapshot,
    state: &LayerState,
    current_layer: usize,
) -> serde_json::Value {
    serde_json::json!({
        "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64(),
        "keyboard": {
            "name": snapshot.definition.name,
            "vendorId": snapshot.definition.vendor_id,
            "productId": snapshot.definition.product_id,
            "uid": snapshot.keyboard_id,
        },
        "protocols": {
            "via": snapshot.via_protocol,
            "vial": snapshot.vial_protocol,
        },
        "state": {
            "effective_layer": state.effective_layer,
            "active_layer": state.active_layer,
            "default_layer": state.default_layer,
            "layer_state": state.layer_state,
            "default_layer_state": state.default_layer_state,
        },
        "layout_options": snapshot.layout_options,
        "definition": &*snapshot.definition,
        "layout": {
            "matrix": {
                "rows": snapshot.definition.matrix.rows,
                "cols": snapshot.definition.matrix.cols,
            },
            "keymap_layout": snapshot.definition.layouts.keymap,
            "layers": snapshot.keymap,
            "keybinds": snapshot.keymap.iter()
                .map(|layer| layer.iter()
                    .map(|row| row.iter()
                        .map(|code| keycode_name(*code))
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "current_layer": current_layer,
            "current_layer_keycodes": snapshot.keymap.get(current_layer).cloned().unwrap_or_default(),
            "current_layer_keybinds": snapshot.keymap.get(current_layer)
                .map(|layer| layer.iter()
                    .map(|row| row.iter()
                        .map(|code| keycode_name(*code))
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>())
                .unwrap_or_default(),
        }
    })
}
