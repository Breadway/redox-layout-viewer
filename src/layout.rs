use eframe::egui::Color32;
use std::collections::HashMap;

use crate::keycode::keycode_name;
use crate::types::{KeyBox, Snapshot};

pub fn iter_keyboxes(layout: &[Vec<serde_json::Value>]) -> Vec<KeyBox> {
    let mut current_y = 0.0;
    let mut rotation = RotationState::default();
    let mut boxes = Vec::new();

    for row in layout {
        let mut x = if rotation.r != 0.0 { rotation.rx } else { 0.0 };
        let mut y = current_y;
        let mut style = StyleState::default();

        for item in row {
            if let Some(obj) = item.as_object() {
                let mut has_origin = false;
                if let Some(value) = obj.get("rx") {
                    rotation.rx = value.as_f64().unwrap_or(0.0) as f32;
                    has_origin = true;
                }
                if let Some(value) = obj.get("ry") {
                    rotation.ry = value.as_f64().unwrap_or(0.0) as f32;
                    has_origin = true;
                }
                if has_origin {
                    x = rotation.rx;
                    y = rotation.ry;
                }
                if let Some(value) = obj.get("r") {
                    rotation.r = value.as_f64().unwrap_or(0.0) as f32;
                }
                if let Some(value) = obj.get("x") {
                    x += value.as_f64().unwrap_or(0.0) as f32;
                }
                if let Some(value) = obj.get("y") {
                    y += value.as_f64().unwrap_or(0.0) as f32;
                }
                if let Some(value) = obj.get("w") {
                    style.w = value.as_f64().unwrap_or(1.0) as f32;
                }
                if let Some(value) = obj.get("h") {
                    style.h = value.as_f64().unwrap_or(1.0) as f32;
                }
                continue;
            }

            if let Some(text) = item.as_str() {
                if text.contains(',') {
                    boxes.push(KeyBox {
                        x,
                        y,
                        w: style.w,
                        h: style.h,
                        r: rotation.r,
                        rx: rotation.rx,
                        ry: rotation.ry,
                        matrix: text.to_string(),
                    });
                    x += style.w;
                    style = StyleState::default();
                }
            }
        }

        current_y = y + 1.0;
    }

    boxes
}

pub fn rotated_box_bounds(box_item: &KeyBox) -> (f32, f32, f32, f32) {
    let x = box_item.x;
    let y = box_item.y;
    let w = box_item.w;
    let h = box_item.h;
    if box_item.r == 0.0 {
        return (x, y, x + w, y + h);
    }

    let angle = box_item.r.to_radians();
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let rx = box_item.rx;
    let ry = box_item.ry;
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for (px, py) in corners {
        let dx = px - rx;
        let dy = py - ry;
        let tx = rx + dx * cos_a - dy * sin_a;
        let ty = ry + dx * sin_a + dy * cos_a;
        min_x = min_x.min(tx);
        min_y = min_y.min(ty);
        max_x = max_x.max(tx);
        max_y = max_y.max(ty);
    }

    (min_x, min_y, max_x, max_y)
}

pub fn parse_matrix_ref(ref_text: &str) -> (usize, usize) {
    let (row, col) = ref_text.split_once(',').expect("invalid matrix ref");
    (row.parse().unwrap(), col.parse().unwrap())
}

pub fn build_fixed_color_overrides(snapshot: &Snapshot) -> HashMap<(usize, usize), Color32> {
    let mut overrides = HashMap::new();
    let Some(base_layer) = snapshot.keymap.first() else {
        return overrides;
    };

    for key_box in &snapshot.keyboxes {
        let (row, col) = parse_matrix_ref(&key_box.matrix);
        let keybind = base_layer
            .get(row)
            .and_then(|row_codes| row_codes.get(col))
            .copied()
            .map(keycode_name)
            .unwrap_or_default();

        if let Some(color) = color_override_from_base_layout(&keybind, row, col, key_box.w) {
            overrides.insert((row, col), color);
        }
    }

    overrides
}

pub fn color_override_from_base_layout(
    keybind: &str,
    row: usize,
    col: usize,
    width: f32,
) -> Option<Color32> {
    if ["KC_1", "KC_2", "KC_3", "KC_4", "KC_5", "KC_LBRC"].contains(&keybind) {
        return Some(Color32::from_rgb(0, 136, 255));
    }
    if (row, col) == (7, 6) || (row, col) == (8, 6) {
        return Some(Color32::from_rgb(0, 136, 255));
    }
    if keybind == "KC_T" {
        return Some(Color32::from_rgb(0, 136, 255));
    }
    if ["KC_0", "KC_6", "KC_7", "KC_8", "KC_9", "KC_RBRC"].contains(&keybind) {
        return Some(Color32::from_rgb(255, 51, 51));
    }
    if keybind == "KC_N" {
        return Some(Color32::from_rgb(255, 51, 51));
    }
    if (row, col) == (2, 6) || (row, col) == (3, 6) {
        return Some(Color32::from_rgb(255, 51, 51));
    }
    if width > 1.0 {
        return Some(Color32::from_rgb(112, 112, 112));
    }
    if (row, col) == (4, 5)
        || (row, col) == (4, 6)
        || (row, col) == (9, 5)
        || (row, col) == (9, 6)
    {
        return Some(Color32::from_rgb(112, 112, 112));
    }
    None
}

#[derive(Default)]
struct RotationState {
    r: f32,
    rx: f32,
    ry: f32,
}

struct StyleState {
    w: f32,
    h: f32,
}

impl Default for StyleState {
    fn default() -> Self {
        Self { w: 1.0, h: 1.0 }
    }
}
