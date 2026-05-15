use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Align2, Color32, FontId, Pos2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::bind::is_keybind_highlighted;
use crate::icons::Icons;
use crate::keycode::{active_label, keycode_name};
use crate::layout::{build_fixed_color_overrides, parse_matrix_ref, rotated_box_bounds};
use crate::render::draw_box;
use crate::snapshot::spawn_snapshot_writer;
use crate::types::{KeyEdge, LayerState, RenderBox, Snapshot};

pub const BACKGROUND_ALPHA: u8 = 0;

pub struct LayoutApp {
    snapshot: Arc<Snapshot>,
    rx: Receiver<LayerState>,
    key_rx: Receiver<KeyEdge>,
    refresh_ms: u64,
    writer_tx: Option<Sender<(LayerState, usize)>>,
    state: LayerState,
    held_binds: HashSet<String>,
    fixed_color_overrides: HashMap<(usize, usize), Color32>,
    render_boxes: Vec<RenderBox>,
    render_bounds: Option<(f32, f32, f32, f32, f32, f32, f32, f32)>,
    /// Cache key for the resolved keymap: (layer_state bitmask, default_layer).
    last_sig: Option<(u32, usize)>,
    icons: Icons,
}

impl LayoutApp {
    pub fn new(
        ctx: &egui::Context,
        snapshot: Arc<Snapshot>,
        rx: Receiver<LayerState>,
        key_rx: Receiver<KeyEdge>,
        refresh_ms: u64,
        output: Option<PathBuf>,
    ) -> Self {
        let fixed_color_overrides = build_fixed_color_overrides(&snapshot);
        let icons = Icons::load(ctx);
        let writer_tx =
            output.map(|path| spawn_snapshot_writer(path, Arc::clone(&snapshot)));
        let mut app = Self {
            snapshot,
            rx,
            key_rx,
            refresh_ms,
            writer_tx,
            state: LayerState::default(),
            held_binds: HashSet::new(),
            fixed_color_overrides,
            render_boxes: Vec::new(),
            render_bounds: None,
            last_sig: None,
            icons,
        };
        app.rebuild_render_cache();
        app.persist_snapshot();
        app
    }

    /// Returns true if the topmost active layer entry for this position is KC_TRNS,
    /// meaning the displayed keycode fell through from a lower layer.
    fn is_transparent(&self, row: usize, col: usize) -> bool {
        const KC_TRNS: u16 = 0x0001;
        let keymap = &self.snapshot.keymap;
        let layer_state = self.state.layer_state;
        let default_layer = self.state.default_layer;
        for l in (0..keymap.len()).rev() {
            let active = (layer_state >> l) & 1 == 1 || l == default_layer;
            if !active {
                continue;
            }
            if let Some(&c) = keymap
                .get(l)
                .and_then(|rows| rows.get(row))
                .and_then(|cols| cols.get(col))
            {
                return c == KC_TRNS;
            }
        }
        false
    }

    /// Resolves the keycode actually in effect at a matrix position, honouring
    /// KC_TRNS fall-through: scan active layers from the top of the stack down;
    /// KC_TRNS (0x0001) passes to the layer below, KC_NO (0x0000) is opaque.
    fn resolve_keycode(&self, row: usize, col: usize) -> u16 {
        const KC_TRNS: u16 = 0x0001;
        let keymap = &self.snapshot.keymap;
        let layer_state = self.state.layer_state;
        let default_layer = self.state.default_layer;
        for l in (0..keymap.len()).rev() {
            let active = (layer_state >> l) & 1 == 1 || l == default_layer;
            if !active {
                continue;
            }
            if let Some(&c) = keymap
                .get(l)
                .and_then(|rows| rows.get(row))
                .and_then(|cols| cols.get(col))
            {
                if c != KC_TRNS {
                    return c;
                }
            }
        }
        keymap
            .get(0)
            .and_then(|rows| rows.get(row))
            .and_then(|cols| cols.get(col))
            .copied()
            .unwrap_or(0)
    }

    fn rebuild_render_cache(&mut self) {
        let sig = (self.state.layer_state, self.state.default_layer);
        if self.last_sig == Some(sig) && !self.render_boxes.is_empty() {
            return;
        }

        if self.snapshot.keymap.is_empty() {
            self.render_boxes.clear();
            self.render_bounds = None;
            return;
        }

        let mut prepared = Vec::with_capacity(self.snapshot.keyboxes.len());
        let mut bounds = Vec::with_capacity(self.snapshot.keyboxes.len());

        for box_item in &self.snapshot.keyboxes {
            let (row, col) = parse_matrix_ref(&box_item.matrix);
            let keybind = keycode_name(self.resolve_keycode(row, col));
            let transparent = self.is_transparent(row, col);
            let fill = self
                .fixed_color_overrides
                .get(&(row, col))
                .copied()
                .unwrap_or(Color32::from_rgb(80, 80, 80));
            prepared.push(RenderBox {
                x: box_item.x,
                y: box_item.y,
                w: box_item.w,
                h: box_item.h,
                rotation: box_item.r,
                rx: box_item.rx,
                ry: box_item.ry,
                fill,
                keybind,
                transparent,
            });
            bounds.push(rotated_box_bounds(box_item));
        }

        let min_x = bounds.iter().map(|b| b.0).fold(f32::INFINITY, f32::min);
        let min_y = bounds.iter().map(|b| b.1).fold(f32::INFINITY, f32::min);
        let max_x = bounds.iter().map(|b| b.2).fold(f32::NEG_INFINITY, f32::max);
        let max_y = bounds.iter().map(|b| b.3).fold(f32::NEG_INFINITY, f32::max);

        let anchor_boxes: Vec<_> = self
            .snapshot
            .keyboxes
            .iter()
            .filter(|b| b.r == 0.0)
            .collect();

        let (ax_min, ay_min, ax_max, ay_max) = if anchor_boxes.is_empty() {
            (min_x, min_y, max_x, max_y)
        } else {
            let ax_min = anchor_boxes.iter().map(|b| b.x).fold(f32::INFINITY, f32::min);
            let ay_min = anchor_boxes.iter().map(|b| b.y).fold(f32::INFINITY, f32::min);
            let ax_max = anchor_boxes.iter().map(|b| b.x + b.w).fold(f32::NEG_INFINITY, f32::max);
            let ay_max = anchor_boxes.iter().map(|b| b.y + b.h).fold(f32::NEG_INFINITY, f32::max);
            (ax_min, ay_min, ax_max, ay_max)
        };

        self.render_boxes = prepared;
        self.render_bounds = Some((min_x, min_y, max_x, max_y, ax_min, ay_min, ax_max, ay_max));
        self.last_sig = Some(sig);
    }

    fn update_state_from_channel(&mut self) {
        let mut changed = false;
        while let Ok(next) = self.rx.try_recv() {
            self.state = next;
            changed = true;
        }
        if changed {
            self.rebuild_render_cache();
            self.persist_snapshot();
        }
    }

    fn update_key_highlights(&mut self) {
        while let Ok(edge) = self.key_rx.try_recv() {
            if edge.pressed {
                self.held_binds.insert(edge.bind);
            } else {
                self.held_binds.remove(&edge.bind);
            }
        }
    }

    fn persist_snapshot(&self) {
        if let Some(tx) = &self.writer_tx {
            let _ = tx.send((self.state.clone(), self.state.effective_layer));
        }
    }
}

impl eframe::App for LayoutApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgba_unmultiplied(0, 0, 0, 0).to_normalized_gamma_f32()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_state_from_channel();
        self.update_key_highlights();

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);
                painter.rect_filled(
                    rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(77, 77, 79, BACKGROUND_ALPHA),
                );

                if self.render_boxes.is_empty() || self.render_bounds.is_none() {
                    painter.text(
                        Pos2::new(40.0, 60.0),
                        Align2::LEFT_TOP,
                        "Waiting for layout...",
                        FontId::proportional(24.0),
                        Color32::from_gray(230),
                    );
                    return;
                }

                let (min_x, min_y, max_x, max_y, ax_min, ay_min, ax_max, ay_max) =
                    self.render_bounds.unwrap();
                let pad = 22.0;
                let scale_x = (rect.width() - pad * 2.0) / (max_x - min_x).max(1.0);
                let scale_y = (rect.height() - pad * 2.0) / (max_y - min_y).max(1.0);
                let scale = scale_x.min(scale_y).max(0.15);
                let anchor_cx = (ax_min + ax_max) * 0.5;
                let anchor_cy = (ay_min + ay_max) * 0.5;
                let oy_bias = 0.12;
                let ox = rect.center().x - anchor_cx * scale;
                let oy = rect.center().y - (anchor_cy + oy_bias) * scale;

                let shift_held = self.held_binds.contains("KC_LSFT")
                    || self.held_binds.contains("KC_RSFT");

                for key_box in &self.render_boxes {
                    let highlighted =
                        is_keybind_highlighted(&key_box.keybind, &self.held_binds);
                    let icon = self.icons.get(&key_box.keybind);
                    let label = active_label(&key_box.keybind, shift_held);
                    draw_box(&painter, key_box, scale, ox, oy, highlighted, icon, &label);
                }
            });

        // Fully event-driven: the HID/evdev reader threads call
        // ctx.request_repaint() on every key edge and layer change, so the UI
        // wakes with zero latency. This is only a slow safety-net fallback in
        // case a reader thread dies.
        ctx.request_repaint_after(Duration::from_millis(self.refresh_ms));
    }
}
