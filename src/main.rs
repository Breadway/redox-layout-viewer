use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, epaint::TextShape, Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use evdev::{enumerate, EventSummary, KeyCode};
use hidapi::{HidApi, HidDevice};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use xz2::read::XzDecoder;

const RAW_USAGE_PAGE: u16 = 0xFF60;
const RAW_USAGE: u16 = 0x61;
const RAW_REPORT_LEN: usize = 32;

const VIAL_PREFIX: u8 = 0xFE;
const CMD_GET_KEYBOARD_ID: u8 = 0x00;
const CMD_GET_SIZE: u8 = 0x01;
const CMD_GET_DEFINITION: u8 = 0x02;

const VIA_GET_PROTOCOL_VERSION: u8 = 0x01;
const VIA_GET_KEYBOARD_VALUE: u8 = 0x02;
const VIA_GET_LAYER_COUNT: u8 = 0x11;
const VIA_GET_KEYMAP_BUFFER: u8 = 0x12;
const VIA_LAYOUT_OPTIONS: u8 = 0x02;

const LAYER_REPORT_ID: u8 = 0x01;
// Set to 0 for testing compositor transparency. Restore to ~179 for 70% opacity.
const BACKGROUND_ALPHA: u8 = 0;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct KeyboardDefinition {
    name: Option<String>,
    #[serde(
        rename = "vendorId",
        default,
        deserialize_with = "deserialize_optional_u16_flexible"
    )]
    vendor_id: Option<u16>,
    #[serde(
        rename = "productId",
        default,
        deserialize_with = "deserialize_optional_u16_flexible"
    )]
    product_id: Option<u16>,
    matrix: MatrixDefinition,
    layouts: LayoutsDefinition,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct MatrixDefinition {
    rows: usize,
    cols: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LayoutsDefinition {
    keymap: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone)]
struct Snapshot {
    keyboard_id: u64,
    vial_protocol: u32,
    via_protocol: u16,
    layout_options: u32,
    definition: Arc<KeyboardDefinition>,
    keymap: Vec<Vec<Vec<u16>>>,
    layer_state: LayerState,
    keyboxes: Vec<KeyBox>,
}

#[derive(Debug, Clone, Default)]
struct LayerState {
    effective_layer: usize,
    active_layer: usize,
    default_layer: usize,
    layer_state: u32,
    default_layer_state: u32,
}

#[derive(Debug, Clone)]
struct KeyBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    rx: f32,
    ry: f32,
    matrix: String,
}

#[derive(Debug, Clone)]
struct RenderBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotation: f32,
    rx: f32,
    ry: f32,
    fill: Color32,
    label: String,
    keybind: String,
}

#[derive(Debug, Clone)]
struct KeyEdge {
    bind: String,
    pressed: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (key_tx, key_rx) = unbounded();

    let api = HidApi::new().context("failed to initialize hidapi")?;
    let device_info = find_raw_device(&api, args.vid, args.pid)?;
    let device = device_info
        .open_device(&api)
        .context("failed to open raw HID device")?;

    let snapshot = Arc::new(load_snapshot(&device, args.output.clone())?);
    let (tx, rx) = unbounded();
    spawn_layer_reader(device, tx)?;
    spawn_global_key_reader(key_tx)?;

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Redox layout")
            .with_app_id("redox-layout-viewer")
            .with_inner_size([1240.0, 520.0])
            .with_transparent(true)
            .with_decorations(false),
        ..Default::default()
    };

    eframe::run_native(
        "Redox layout",
        native_options,
        Box::new(|_cc| {
            Ok(Box::new(LayoutApp::new(
                snapshot,
                rx,
                key_rx,
                args.refresh_ms,
                args.output,
            )))
        }),
    )
    .map_err(|err| anyhow!(err.to_string()))
}

#[derive(Debug, Clone)]
struct Args {
    vid: u16,
    pid: u16,
    output: Option<PathBuf>,
    refresh_ms: u64,
}

impl Args {
    fn parse() -> Self {
        let mut vid = 0x4D44;
        let mut pid = 0x5244;
        let mut output = None;
        let mut refresh_ms = 250;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--vid" => {
                    vid = parse_int(&args.next().expect("missing value for --vid"));
                }
                "--pid" => {
                    pid = parse_int(&args.next().expect("missing value for --pid"));
                }
                "--output" => {
                    output = Some(PathBuf::from(
                        args.next().expect("missing value for --output"),
                    ));
                }
                "--refresh-ms" => {
                    refresh_ms = args
                        .next()
                        .expect("missing value for --refresh-ms")
                        .parse()
                        .expect("invalid --refresh-ms");
                }
                "--once" => {}
                _ => panic!("unknown argument: {arg}"),
            }
        }

        Self {
            vid,
            pid,
            output,
            refresh_ms,
        }
    }
}

fn parse_int(text: &str) -> u16 {
    if let Some(hex) = text.strip_prefix("0x") {
        u16::from_str_radix(hex, 16).expect("invalid hex value")
    } else {
        text.parse().expect("invalid integer value")
    }
}

fn deserialize_optional_u16_flexible<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = raw else {
        return Ok(None);
    };

    match value {
        serde_json::Value::Number(number) => {
            let parsed = number
                .as_u64()
                .ok_or_else(|| de::Error::custom("expected unsigned integer"))?;
            u16::try_from(parsed)
                .map(Some)
                .map_err(|_| de::Error::custom("value out of range for u16"))
        }
        serde_json::Value::String(text) => {
            let parsed = if let Some(hex) = text.strip_prefix("0x") {
                u16::from_str_radix(hex, 16)
            } else {
                text.parse::<u16>()
            }
            .map_err(|_| de::Error::custom("invalid u16 string value"))?;
            Ok(Some(parsed))
        }
        _ => Err(de::Error::custom(
            "expected vendor/product ID as integer or string",
        )),
    }
}

fn find_raw_device(api: &HidApi, vid: u16, pid: u16) -> Result<hidapi::DeviceInfo> {
    api.device_list()
        .find(|dev| {
            dev.vendor_id() == vid
                && dev.product_id() == pid
                && dev.usage_page() == RAW_USAGE_PAGE
                && dev.usage() == RAW_USAGE
        })
        .cloned()
        .ok_or_else(|| anyhow!("No matching raw HID interface found"))
}

fn spawn_layer_reader(device: HidDevice, tx: Sender<LayerState>) -> Result<()> {
    thread::Builder::new()
        .name("layer-reader".into())
        .spawn(move || loop {
            let mut buf = [0u8; RAW_REPORT_LEN];
            match device.read_timeout(&mut buf, 1000) {
                Ok(0) => continue,
                Ok(_) => {
                    if let Some(report) = parse_layer_report(&buf) {
                        let _ = tx.send(report);
                    }
                }
                Err(_) => break,
            }
        })
        .context("failed to spawn layer reader thread")?;
    Ok(())
}

fn load_snapshot(device: &HidDevice, output: Option<PathBuf>) -> Result<Snapshot> {
    let (keyboard_id, vial_protocol) = get_keyboard_id_and_protocol(device)?;
    let definition = Arc::new(get_definition(device)?);
    let via_protocol = get_via_protocol(device)?;
    let layer_count = get_layer_count(device)? as usize;
    let layout_options = get_layout_options(device)?;
    let keymap = get_keymap(
        device,
        layer_count,
        definition.matrix.rows,
        definition.matrix.cols,
    )?;
    let keyboxes = iter_keyboxes(&definition.layouts.keymap);

    let snapshot = Snapshot {
        keyboard_id,
        vial_protocol,
        via_protocol,
        layout_options,
        definition,
        keymap,
        layer_state: LayerState::default(),
        keyboxes,
    };

    if let Some(path) = output {
        write_snapshot(&path, &snapshot, &snapshot.layer_state, 0)?;
    }

    Ok(snapshot)
}

fn write_snapshot(
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

fn snapshot_to_json(
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
            "keybinds": snapshot
                .keymap
                .iter()
                .map(|layer| layer.iter().map(|row| row.iter().map(|code| keycode_name(*code)).collect::<Vec<_>>()).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "current_layer": current_layer,
            "current_layer_keycodes": snapshot.keymap.get(current_layer).cloned().unwrap_or_default(),
            "current_layer_keybinds": snapshot
                .keymap
                .get(current_layer)
                .map(|layer| layer.iter().map(|row| row.iter().map(|code| keycode_name(*code)).collect::<Vec<_>>()).collect::<Vec<_>>())
                .unwrap_or_default(),
        }
    })
}

fn get_keyboard_id_and_protocol(device: &HidDevice) -> Result<(u64, u32)> {
    let data = request(device, &[VIAL_PREFIX, CMD_GET_KEYBOARD_ID])?;
    let vial_protocol = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let keyboard_id = u64::from_le_bytes(data[4..12].try_into().unwrap());
    Ok((keyboard_id, vial_protocol))
}

fn get_definition(device: &HidDevice) -> Result<KeyboardDefinition> {
    let size_data = request(device, &[VIAL_PREFIX, CMD_GET_SIZE])?;
    let size = u32::from_le_bytes(size_data[0..4].try_into().unwrap()) as usize;
    let mut payload = Vec::with_capacity(size);
    let mut block = 0u32;
    let mut remaining = size;

    while remaining > 0 {
        let req = [
            VIAL_PREFIX,
            CMD_GET_DEFINITION,
            (block & 0xFF) as u8,
            ((block >> 8) & 0xFF) as u8,
            ((block >> 16) & 0xFF) as u8,
            ((block >> 24) & 0xFF) as u8,
        ];
        let data = request(device, &req)?;
        let take = remaining.min(RAW_REPORT_LEN);
        payload.extend_from_slice(&data[..take]);
        remaining -= take;
        block += 1;
    }

    let mut decoder = XzDecoder::new(&payload[..]);
    let mut json = String::new();
    decoder.read_to_string(&mut json)?;
    Ok(serde_json::from_str(&json)?)
}

fn get_via_protocol(device: &HidDevice) -> Result<u16> {
    let data = request(device, &[VIA_GET_PROTOCOL_VERSION])?;
    Ok(u16::from_be_bytes([data[1], data[2]]))
}

fn get_layer_count(device: &HidDevice) -> Result<u8> {
    let data = request(device, &[VIA_GET_LAYER_COUNT])?;
    Ok(data[1])
}

fn get_layout_options(device: &HidDevice) -> Result<u32> {
    let data = request(device, &[VIA_GET_KEYBOARD_VALUE, VIA_LAYOUT_OPTIONS])?;
    Ok(u32::from_be_bytes([data[2], data[3], data[4], data[5]]))
}

fn get_keymap(
    device: &HidDevice,
    layer_count: usize,
    rows: usize,
    cols: usize,
) -> Result<Vec<Vec<Vec<u16>>>> {
    let total = layer_count * rows * cols * 2;
    let mut buf = Vec::with_capacity(total);

    for offset in (0..total).step_by(28) {
        let size = (total - offset).min(28);
        let req = [
            VIA_GET_KEYMAP_BUFFER,
            ((offset >> 8) & 0xFF) as u8,
            (offset & 0xFF) as u8,
            size as u8,
        ];
        let data = request(device, &req)?;
        buf.extend_from_slice(&data[4..4 + size]);
    }

    let mut layers = Vec::with_capacity(layer_count);
    let mut idx = 0;
    for _ in 0..layer_count {
        let mut layer = Vec::with_capacity(rows);
        for _ in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for _ in 0..cols {
                let code = u16::from_be_bytes([buf[idx], buf[idx + 1]]);
                row.push(code);
                idx += 2;
            }
            layer.push(row);
        }
        layers.push(layer);
    }
    Ok(layers)
}

fn request(device: &HidDevice, payload: &[u8]) -> Result<[u8; RAW_REPORT_LEN]> {
    if payload.len() > RAW_REPORT_LEN {
        return Err(anyhow!("payload too large"));
    }
    let mut report = [0u8; RAW_REPORT_LEN + 1];
    report[1..1 + payload.len()].copy_from_slice(payload);
    device.write(&report)?;

    let mut read_buf = [0u8; RAW_REPORT_LEN];
    let read_len = device.read_timeout(&mut read_buf, 1000)?;
    if read_len == 0 {
        return Err(anyhow!("hid timeout"));
    }
    Ok(read_buf)
}

fn parse_layer_report(data: &[u8; RAW_REPORT_LEN]) -> Option<LayerState> {
    if data[0] != LAYER_REPORT_ID {
        return None;
    }
    Some(LayerState {
        effective_layer: data[1] as usize,
        active_layer: data[2] as usize,
        default_layer: data[3] as usize,
        layer_state: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
        default_layer_state: u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
    })
}

fn iter_keyboxes(layout: &[Vec<serde_json::Value>]) -> Vec<KeyBox> {
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
        Self {
            w: 1.0,
            h: 1.0,
        }
    }
}

struct LayoutApp {
    snapshot: Arc<Snapshot>,
    rx: Receiver<LayerState>,
    key_rx: Receiver<KeyEdge>,
    refresh_ms: u64,
    output: Option<PathBuf>,
    state: LayerState,
    highlight_binds: HashSet<String>,
    fixed_color_overrides: HashMap<(usize, usize), Color32>,
    render_boxes: Vec<RenderBox>,
    render_bounds: Option<(f32, f32, f32, f32, f32, f32, f32, f32)>,
    last_rebuild_layer: usize,
}

impl LayoutApp {
    fn new(
        snapshot: Arc<Snapshot>,
        rx: Receiver<LayerState>,
        key_rx: Receiver<KeyEdge>,
        refresh_ms: u64,
        output: Option<PathBuf>,
    ) -> Self {
        let fixed_color_overrides = build_fixed_color_overrides(&snapshot);
        let mut app = Self {
            snapshot,
            rx,
            key_rx,
            refresh_ms,
            output,
            state: LayerState::default(),
            highlight_binds: HashSet::new(),
            fixed_color_overrides,
            render_boxes: Vec::new(),
            render_bounds: None,
            last_rebuild_layer: usize::MAX,
        };
        app.rebuild_render_cache();
        app.persist_snapshot();
        app
    }

    fn rebuild_render_cache(&mut self) {
        let layer_index = self
            .state
            .effective_layer
            .min(self.snapshot.keymap.len().saturating_sub(1));
        if self.last_rebuild_layer == layer_index && !self.render_boxes.is_empty() {
            return;
        }

        let keybinds = match self.snapshot.keymap.get(layer_index) {
            Some(layer) => layer,
            None => {
                self.render_boxes.clear();
                self.render_bounds = None;
                return;
            }
        };

        let mut prepared = Vec::with_capacity(self.snapshot.keyboxes.len());
        let mut bounds = Vec::with_capacity(self.snapshot.keyboxes.len());

        for box_item in &self.snapshot.keyboxes {
            let (row, col) = parse_matrix_ref(&box_item.matrix);
            let keycode = keybinds
                .get(row)
                .and_then(|row_codes| row_codes.get(col))
                .copied();
            let keybind = keycode.map(keycode_name).unwrap_or_default();
            let label = pretty_bind_label(&keybind);
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
                label,
                keybind,
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
            .filter(|box_item| box_item.r == 0.0)
            .collect();

        let (ax_min, ay_min, ax_max, ay_max) = if anchor_boxes.is_empty() {
            (min_x, min_y, max_x, max_y)
        } else {
            let ax_min = anchor_boxes
                .iter()
                .map(|box_item| box_item.x)
                .fold(f32::INFINITY, f32::min);
            let ay_min = anchor_boxes
                .iter()
                .map(|box_item| box_item.y)
                .fold(f32::INFINITY, f32::min);
            let ax_max = anchor_boxes
                .iter()
                .map(|box_item| box_item.x + box_item.w)
                .fold(f32::NEG_INFINITY, f32::max);
            let ay_max = anchor_boxes
                .iter()
                .map(|box_item| box_item.y + box_item.h)
                .fold(f32::NEG_INFINITY, f32::max);
            (ax_min, ay_min, ax_max, ay_max)
        };

        self.render_boxes = prepared;
        self.render_bounds = Some((min_x, min_y, max_x, max_y, ax_min, ay_min, ax_max, ay_max));
        self.last_rebuild_layer = layer_index;
    }

    fn update_state_from_channel(&mut self) {
        while let Ok(next) = self.rx.try_recv() {
            self.state = next;
            self.rebuild_render_cache();
            self.persist_snapshot();
        }
    }

    fn update_key_highlights(&mut self) {
        while let Ok(edge) = self.key_rx.try_recv() {
            if edge.pressed {
                self.highlight_binds.insert(edge.bind);
            } else {
                self.highlight_binds.remove(&edge.bind);
            }
        }
    }

    fn persist_snapshot(&self) {
        if let Some(path) = &self.output {
            let _ = write_snapshot(
                path,
                &self.snapshot,
                &self.state,
                self.state.effective_layer,
            );
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

            for key_box in &self.render_boxes {
                draw_box(
                    &painter,
                    key_box,
                    scale,
                    ox,
                    oy,
                    self.highlight_binds.contains(&key_box.keybind),
                );
            }
        });

        ctx.request_repaint_after(Duration::from_millis(self.refresh_ms));
    }
}

fn spawn_global_key_reader(tx: Sender<KeyEdge>) -> Result<()> {
    thread::Builder::new()
        .name("global-key-reader".into())
        .spawn(move || {
            eprintln!("[EVDEV] Global key reader thread started");
            let mut device_count = 0;
            for (path, mut device) in enumerate() {
                device_count += 1;
                eprintln!("[EVDEV] Found device #{}: {} (path: {:?})", device_count, device.name().unwrap_or("unknown"), path);

                if device.supported_keys().is_none() {
                    eprintln!("[EVDEV]   -> No supported keys, skipping");
                    continue;
                }
                eprintln!("[EVDEV]   -> Has supported keys, spawning reader");

                let tx = tx.clone();
                let device_name = device.name().unwrap_or("unknown").to_string();
                let device_name_clone = device_name.clone();
                let spawn_result = thread::Builder::new()
                    .name(format!("evdev-{device_name}"))
                    .spawn(move || loop {
                        match device.fetch_events() {
                            Ok(events) => {
                                for event in events {
                                    match event.destructure() {
                                        EventSummary::Key(_, key_code, 1 | 2) => {
                                            if let Some(bind) = evdev_keycode_to_bind(key_code) {
                                                eprintln!("[EVDEV] Key pressed: {:?} -> {}", key_code, bind);
                                                let _ = tx.send(KeyEdge { bind, pressed: true });
                                            }
                                        }
                                        EventSummary::Key(_, key_code, 0) => {
                                            if let Some(bind) = evdev_keycode_to_bind(key_code) {
                                                eprintln!("[EVDEV] Key released: {:?} -> {}", key_code, bind);
                                                let _ = tx.send(KeyEdge {
                                                    bind,
                                                    pressed: false,
                                                });
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[EVDEV] Error reading from {}: {}", device_name_clone, e);
                                break;
                            }
                        }
                    });
                
                if let Err(e) = spawn_result {
                    eprintln!("[EVDEV] Failed to spawn reader for {}: {}", device_name, e);
                }
            }
            eprintln!("[EVDEV] Enumeration complete. Found {} total devices.", device_count);
        })
        .context("failed to spawn global key reader thread")?;
    Ok(())
}

fn evdev_keycode_to_bind(key_code: KeyCode) -> Option<String> {
    let name = format!("{key_code:?}");
    let bind = match name.as_str() {
        "KEY_ESC" => "KC_ESC",
        "KEY_TAB" => "KC_TAB",
        "KEY_ENTER" => "KC_ENT",
        "KEY_SPACE" => "KC_SPC",
        "KEY_BACKSPACE" => "KC_BSPC",
        "KEY_INSERT" => "KC_INS",
        "KEY_DELETE" => "KC_DEL",
        "KEY_HOME" => "KC_HOME",
        "KEY_END" => "KC_END",
        "KEY_PAGEUP" => "KC_PGUP",
        "KEY_PAGEDOWN" => "KC_PGDN",
        "KEY_LEFT" => "KC_LEFT",
        "KEY_RIGHT" => "KC_RGHT",
        "KEY_UP" => "KC_UP",
        "KEY_DOWN" => "KC_DOWN",
        "KEY_GRAVE" => "KC_GRV",
        "KEY_MINUS" => "KC_MINS",
        "KEY_EQUAL" => "KC_EQL",
        "KEY_LEFTBRACE" => "KC_LBRC",
        "KEY_RIGHTBRACE" => "KC_RBRC",
        "KEY_BACKSLASH" => "KC_BSLS",
        "KEY_SEMICOLON" => "KC_SCLN",
        "KEY_APOSTROPHE" => "KC_QUOT",
        "KEY_COMMA" => "KC_COMM",
        "KEY_DOT" => "KC_DOT",
        "KEY_SLASH" => "KC_SLSH",
        "KEY_CAPSLOCK" => "KC_CAPS",
        "KEY_NUMLOCK" => "KC_NLCK",
        "KEY_PRINT" => "KC_PSCR",
        "KEY_SCROLLLOCK" => "KC_SCRL",
        "KEY_PAUSE" => "KC_PAUS",
        "KEY_LEFTSHIFT" => "KC_LSFT",
        "KEY_RIGHTSHIFT" => "KC_RSFT",
        "KEY_LEFTCTRL" => "KC_LCTL",
        "KEY_RIGHTCTRL" => "KC_RCTL",
        "KEY_LEFTALT" => "KC_LALT",
        "KEY_RIGHTALT" => "KC_RALT",
        "KEY_LEFTMETA" => "KC_LGUI",
        "KEY_RIGHTMETA" => "KC_RGUI",
        "KEY_KPENTER" => "KC_PENT",
        "KEY_KPSLASH" => "KC_PSLH",
        "KEY_KPASTERISK" => "KC_PAST",
        "KEY_KPMINUS" => "KC_PMNS",
        "KEY_KPPLUS" => "KC_PPLS",
        "KEY_KPDOT" => "KC_PDOT",
        "KEY_KP0" => "KC_P0",
        "KEY_KP1" => "KC_P1",
        "KEY_KP2" => "KC_P2",
        "KEY_KP3" => "KC_P3",
        "KEY_KP4" => "KC_P4",
        "KEY_KP5" => "KC_P5",
        "KEY_KP6" => "KC_P6",
        "KEY_KP7" => "KC_P7",
        "KEY_KP8" => "KC_P8",
        "KEY_KP9" => "KC_P9",
        _ => {
            if let Some(suffix) = name.strip_prefix("KEY_") {
                if suffix.len() == 1 {
                    if suffix.chars().all(|ch| ch.is_ascii_uppercase()) {
                        return Some(format!("KC_{suffix}"));
                    }
                    if suffix.chars().all(|ch| ch.is_ascii_digit()) {
                        return Some(format!("KC_{suffix}"));
                    }
                }
                if let Some(number) = suffix.strip_prefix('F') {
                    if number.chars().all(|ch| ch.is_ascii_digit()) {
                        return Some(format!("KC_F{number}"));
                    }
                }
            }
            return None;
        }
    };
    Some(bind.to_string())
}

fn draw_box(
    painter: &egui::Painter,
    key_box: &RenderBox,
    scale: f32,
    ox: f32,
    oy: f32,
    highlighted: bool,
) {
    let fill = if highlighted {
        Color32::WHITE
    } else {
        key_box.fill
    };
    let stroke = if highlighted {
        Stroke::new(2.0, Color32::WHITE)
    } else {
        Stroke::new(1.0, Color32::from_rgb(54, 56, 61))
    };
    let radius = (key_box.w.min(key_box.h) * scale * 0.14).max(2.0);

    let rect = Rect::from_min_size(
        Pos2::new(ox + key_box.x * scale, oy + key_box.y * scale),
        Vec2::new(key_box.w * scale, key_box.h * scale),
    );

    let has_rotation = key_box.rotation.abs() > f32::EPSILON;
    let origin = Pos2::new(ox + key_box.rx * scale, oy + key_box.ry * scale);
    let angle = key_box.rotation.to_radians();

    if has_rotation {
        let points = rounded_rect_points(rect, radius, origin, angle);
        painter.add(Shape::convex_polygon(points, fill, stroke));
    } else {
        painter.rect(rect, radius, fill, stroke);
    }

    if !key_box.label.is_empty() {
        let text_color = if luminance(fill) < 0.55 {
            Color32::from_rgb(247, 247, 250)
        } else {
            Color32::from_rgb(36, 36, 36)
        };
        let lines: Vec<&str> = key_box.label.lines().collect();
        let max_lines = lines.len().max(1) as f32;
        let mut font_size = if key_box.label.len() <= 3 {
            16.0
        } else if key_box.label.len() > 10 {
            10.0
        } else {
            13.0
        };

        let max_text_width = rect.width() * 0.86;
        let max_text_height = rect.height() * 0.86;
        let min_font_size = 6.0;

        let base_font = FontId::proportional(font_size);
        let max_line_width = lines
            .iter()
            .map(|line| {
                painter
                    .layout_no_wrap((*line).to_string(), base_font.clone(), text_color)
                    .size()
                    .x
            })
            .fold(0.0, f32::max);

        if max_line_width > max_text_width && max_line_width > 0.0 {
            let shrink = max_text_width / max_line_width;
            font_size = (font_size * shrink).max(min_font_size);
        }

        let estimated_total_height = max_lines * font_size * 1.15;
        if estimated_total_height > max_text_height && estimated_total_height > 0.0 {
            let shrink = max_text_height / estimated_total_height;
            font_size = (font_size * shrink).max(min_font_size);
        }

        let total_height = max_lines * font_size * 1.15;
        let mut y = rect.center().y - total_height * 0.5;
        let line_font = FontId::proportional(font_size);
        for line in lines {
            let galley = painter.layout_no_wrap(line.to_string(), line_font.clone(), text_color);
            let top_left = Pos2::new(rect.center().x - galley.size().x * 0.5, y);
            if has_rotation {
                let rotated_top_left = rotate_point(top_left, origin, angle);
                let text_shape = TextShape::new(rotated_top_left, galley, text_color)
                    .with_override_text_color(text_color)
                    .with_angle(angle);
                painter.add(Shape::Text(text_shape));
            } else {
                painter.galley(top_left, galley, text_color);
            }
            y += font_size * 1.15;
        }
    }
}

fn rounded_rect_points(rect: Rect, _radius: f32, origin: Pos2, angle: f32) -> Vec<Pos2> {
    let width = rect.width();
    let height = rect.height();
    let radius = _radius.min(width * 0.5).min(height * 0.5);

    let mut points = Vec::new();
    if radius <= 0.0 {
        let corners = [
            Pos2::new(rect.right(), rect.top()),
            Pos2::new(rect.right(), rect.bottom()),
            Pos2::new(rect.left(), rect.bottom()),
            Pos2::new(rect.left(), rect.top()),
        ];
        points.extend(corners.into_iter().map(|point| rotate_point(point, origin, angle)));
        return points;
    }

    let arc_steps = 6;
    let quarter = std::f32::consts::FRAC_PI_2;
    let centers = [
        (Pos2::new(rect.right() - radius, rect.top() + radius), -quarter, 0.0),
        (Pos2::new(rect.right() - radius, rect.bottom() - radius), 0.0, quarter),
        (
            Pos2::new(rect.left() + radius, rect.bottom() - radius),
            quarter,
            std::f32::consts::PI,
        ),
        (
            Pos2::new(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
            std::f32::consts::PI + quarter,
        ),
    ];

    for (idx, (center, start, end)) in centers.iter().enumerate() {
        let start_step = if idx == 0 { 0 } else { 1 };
        for step in start_step..=arc_steps {
            let t = step as f32 / arc_steps as f32;
            let theta = start + (end - start) * t;
            let point = Pos2::new(center.x + radius * theta.cos(), center.y + radius * theta.sin());
            points.push(rotate_point(point, origin, angle));
        }
    }

    points
}

fn rotate_point(point: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let sin = angle.sin();
    let cos = angle.cos();
    let dx = point.x - origin.x;
    let dy = point.y - origin.y;
    Pos2::new(
        origin.x + dx * cos - dy * sin,
        origin.y + dx * sin + dy * cos,
    )
}

fn luminance(color: Color32) -> f32 {
    (0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32) / 255.0
}

fn build_fixed_color_overrides(snapshot: &Snapshot) -> HashMap<(usize, usize), Color32> {
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

fn color_override_from_base_layout(
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
    if (row, col) == (4, 5) || (row, col) == (4, 6) || (row, col) == (9, 5) || (row, col) == (9, 6)
    {
        return Some(Color32::from_rgb(112, 112, 112));
    }
    None
}

fn parse_matrix_ref(ref_text: &str) -> (usize, usize) {
    let (row, col) = ref_text.split_once(',').expect("invalid matrix ref");
    (row.parse().unwrap(), col.parse().unwrap())
}

fn pretty_bind_label(bind: &str) -> String {
    if bind == "KC_TRNS" || bind == "KC_NO" {
        return String::new();
    }

    let shifted_pair = match bind {
        "KC_GRV" => Some("~\n`"),
        "KC_1" => Some("!\n1"),
        "KC_2" => Some("@\n2"),
        "KC_3" => Some("#\n3"),
        "KC_4" => Some("$\n4"),
        "KC_5" => Some("%\n5"),
        "KC_6" => Some("^\n6"),
        "KC_7" => Some("&\n7"),
        "KC_8" => Some("*\n8"),
        "KC_9" => Some("(\n9"),
        "KC_0" => Some(")\n0"),
        "KC_MINS" => Some("_\n-"),
        "KC_EQL" => Some("+\n="),
        "KC_LBRC" => Some("{\n["),
        "KC_RBRC" => Some("}\n]"),
        "KC_BSLS" => Some("|\n\\"),
        "KC_SCLN" => Some(":\n;"),
        "KC_QUOT" => Some("\"\n'"),
        "KC_COMM" => Some("<\n,"),
        "KC_DOT" => Some(">\n."),
        "KC_SLSH" => Some("?\n/"),
        _ => None,
    };
    if let Some(label) = shifted_pair {
        return label.to_string();
    }

    let mapping = match bind {
        "KC_TAB" => Some("Tab"),
        "KC_ESC" => Some("Esc"),
        "KC_BSPC" => Some("Bksp"),
        "KC_SPC" => Some("Space"),
        "KC_LSFT" => Some("LShift"),
        "KC_RSFT" => Some("RShift"),
        "KC_LCTL" => Some("LCtrl"),
        "KC_RCTL" => Some("RCtrl"),
        "KC_LALT" => Some("LAlt"),
        "KC_RALT" => Some("RAlt"),
        "KC_LGUI" => Some("LGui"),
        "KC_RGUI" => Some("RGui"),
        "KC_PSCR" => Some("Print Screen"),
        "KC_SCRL" => Some("Scroll Lock"),
        "KC_PAUS" => Some("Pause"),
        "KC_INS" => Some("Ins"),
        "KC_DEL" => Some("Del"),
        "KC_HOME" => Some("Home"),
        "KC_END" => Some("End"),
        "KC_PGUP" => Some("PgUp"),
        "KC_PGDN" => Some("PgDn"),
        "KC_LEFT" => Some("Left"),
        "KC_DOWN" => Some("Down"),
        "KC_UP" => Some("Up"),
        "KC_RGHT" => Some("Right"),
        "KC_LBRC" => Some("["),
        "KC_RBRC" => Some("]"),
        "KC_BSLS" => Some("\\"),
        "KC_GRV" => Some("~"),
        "KC_MINS" => Some("-"),
        "KC_EQL" => Some("="),
        "KC_SCLN" => Some(";"),
        "KC_QUOT" => Some("'"),
        "KC_COMM" => Some(","),
        "KC_DOT" => Some("."),
        "KC_SLSH" => Some("/"),
        "KC_EXLM" => Some("!"),
        "KC_AT" => Some("@"),
        "KC_HASH" => Some("#"),
        "KC_DLR" => Some("$"),
        "KC_PERC" => Some("%"),
        "KC_CIRC" => Some("^"),
        "KC_AMPR" => Some("&"),
        "KC_ASTR" => Some("*"),
        "KC_LPRN" => Some("("),
        "KC_RPRN" => Some(")"),
        "KC_UNDS" => Some("_"),
        "KC_PLUS" => Some("+"),
        "KC_LCBR" => Some("{"),
        "KC_RCBR" => Some("}"),
        "KC_PIPE" => Some("|"),
        "KC_COLN" => Some(":"),
        "KC_DQUO" => Some("\""),
        "KC_TILD" => Some("~"),
        "KC_LABK" => Some("<"),
        "KC_RABK" => Some(">"),
        "KC_QUES" => Some("?"),
        "KC_CAPS" => Some("Caps"),
        "KC_NLCK" => Some("Num"),
        "KC_PSLH" => Some("/"),
        "KC_PAST" => Some("*"),
        "KC_PMNS" => Some("-"),
        "KC_PPLS" => Some("+"),
        "KC_PENT" => Some("Enter"),
        "KC_PDOT" => Some("."),
        "KC_NUBS" => Some("\\"),
        "KC_MUTE" => Some("Mute"),
        "KC_VOLU" => Some("Vol+"),
        "KC_VOLD" => Some("Vol-"),
        "KC_MNXT" => Some("Next"),
        "KC_MPRV" => Some("Prev"),
        "KC_MSTP" => Some("Stop"),
        "KC_MPLY" => Some("Play"),
        "KC_MSEL" => Some("Sel"),
        "KC_EJCT" => Some("Eject"),
        _ => None,
    };
    if let Some(text) = mapping {
        return text.to_string();
    }

    if let Some(rest) = bind.strip_prefix("KC_") {
        if rest.len() == 1 {
            return rest.to_string();
        }
        if let Some(num) = rest.strip_prefix('F') {
            if num.chars().all(|ch| ch.is_ascii_digit()) {
                return num.to_string();
            }
        }
        return rest.to_string();
    }

    if let Some(inner) = bind.strip_prefix("LGUI(").and_then(|s| s.strip_suffix(')')) {
        return format!("LGui+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("LCTL(").and_then(|s| s.strip_suffix(')')) {
        return format!("LCtrl+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("LSFT(").and_then(|s| s.strip_suffix(')')) {
        return format!("LShift+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("LALT(").and_then(|s| s.strip_suffix(')')) {
        return format!("LAlt+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind.strip_prefix("RGUI(").and_then(|s| s.strip_suffix(')')) {
        return format!("RGui+{}", pretty_bind_label(inner));
    }
    if let Some(inner) = bind
        .strip_prefix("RCtrl(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return format!("RCtrl+{}", pretty_bind_label(inner));
    }
    bind.to_string()
}

fn mod_names(mods: u16) -> Vec<&'static str> {
    if mods == 0x0F || mods == 0x1F {
        return vec!["HYPR"];
    }
    if mods == 0x07 {
        return vec!["MEH"];
    }

    let side_bits: &[(u16, &str)] = if (mods & 0x10) != 0 {
        &[(0x01, "RCTL"), (0x02, "RSFT"), (0x04, "RALT"), (0x08, "RGUI")]
    } else {
        &[(0x01, "LCTL"), (0x02, "LSFT"), (0x04, "LALT"), (0x08, "LGUI")]
    };

    side_bits
        .iter()
        .filter_map(|(bit, name)| ((mods & bit) != 0).then_some(*name))
        .collect()
}

fn format_mods(mods: u16) -> String {
    let names = mod_names(mods);
    if names.is_empty() {
        format!("0x{mods:X}")
    } else {
        names.join("|")
    }
}

fn wrap_mods(base: String, mods: u16) -> String {
    let names = mod_names(mods);
    if names.is_empty() {
        return base;
    }
    names
        .iter()
        .rev()
        .fold(base, |acc, name| format!("{name}({acc})"))
}

fn us_shift_alias(base: u16) -> Option<&'static str> {
    match base {
        0x1E => Some("KC_EXLM"),
        0x1F => Some("KC_AT"),
        0x20 => Some("KC_HASH"),
        0x21 => Some("KC_DLR"),
        0x22 => Some("KC_PERC"),
        0x23 => Some("KC_CIRC"),
        0x24 => Some("KC_AMPR"),
        0x25 => Some("KC_ASTR"),
        0x26 => Some("KC_LPRN"),
        0x27 => Some("KC_RPRN"),
        0x2D => Some("KC_UNDS"),
        0x2E => Some("KC_PLUS"),
        0x2F => Some("KC_LCBR"),
        0x30 => Some("KC_RCBR"),
        0x31 => Some("KC_PIPE"),
        0x33 => Some("KC_COLN"),
        0x34 => Some("KC_DQUO"),
        0x35 => Some("KC_TILD"),
        0x36 => Some("KC_LABK"),
        0x37 => Some("KC_RABK"),
        0x38 => Some("KC_QUES"),
        _ => None,
    }
}

fn single_mod_alias(mods: u16) -> Option<&'static str> {
    match mods {
        0x01 => Some("LCTL"),
        0x02 => Some("LSFT"),
        0x04 => Some("LALT"),
        0x08 => Some("LGUI"),
        0x11 => Some("RCTL"),
        0x12 => Some("RSFT"),
        0x14 => Some("RALT"),
        0x18 => Some("RGUI"),
        _ => None,
    }
}

fn special_magic_name(keycode: u16) -> Option<&'static str> {
    match keycode {
        0x7000 => Some("QK_MAGIC_SWAP_CONTROL_CAPS_LOCK"),
        0x7001 => Some("QK_MAGIC_UNSWAP_CONTROL_CAPS_LOCK"),
        0x7002 => Some("QK_MAGIC_TOGGLE_CONTROL_CAPS_LOCK"),
        0x7003 => Some("QK_MAGIC_CAPS_LOCK_AS_CONTROL_OFF"),
        0x7004 => Some("QK_MAGIC_CAPS_LOCK_AS_CONTROL_ON"),
        0x7005 => Some("QK_MAGIC_SWAP_LALT_LGUI"),
        0x7006 => Some("QK_MAGIC_UNSWAP_LALT_LGUI"),
        0x7007 => Some("QK_MAGIC_SWAP_RALT_RGUI"),
        0x7008 => Some("QK_MAGIC_UNSWAP_RALT_RGUI"),
        0x7009 => Some("QK_MAGIC_GUI_ON"),
        0x700A => Some("QK_MAGIC_GUI_OFF"),
        0x700B => Some("QK_MAGIC_TOGGLE_GUI"),
        0x700C => Some("QK_MAGIC_SWAP_GRAVE_ESC"),
        0x700D => Some("QK_MAGIC_UNSWAP_GRAVE_ESC"),
        0x700E => Some("QK_MAGIC_SWAP_BACKSLASH_BACKSPACE"),
        0x700F => Some("QK_MAGIC_UNSWAP_BACKSLASH_BACKSPACE"),
        0x7010 => Some("QK_MAGIC_TOGGLE_BACKSLASH_BACKSPACE"),
        0x7011 => Some("QK_MAGIC_NKRO_ON"),
        0x7012 => Some("QK_MAGIC_NKRO_OFF"),
        0x7013 => Some("QK_MAGIC_TOGGLE_NKRO"),
        0x7014 => Some("QK_MAGIC_SWAP_ALT_GUI"),
        0x7015 => Some("QK_MAGIC_UNSWAP_ALT_GUI"),
        0x7016 => Some("QK_MAGIC_TOGGLE_ALT_GUI"),
        0x7017 => Some("QK_MAGIC_SWAP_LCTL_LGUI"),
        0x7018 => Some("QK_MAGIC_UNSWAP_LCTL_LGUI"),
        0x7019 => Some("QK_MAGIC_SWAP_RCTL_RGUI"),
        0x701A => Some("QK_MAGIC_UNSWAP_RCTL_RGUI"),
        0x701B => Some("QK_MAGIC_SWAP_CTL_GUI"),
        0x701C => Some("QK_MAGIC_UNSWAP_CTL_GUI"),
        0x701D => Some("QK_MAGIC_TOGGLE_CTL_GUI"),
        0x701E => Some("QK_MAGIC_EE_HANDS_LEFT"),
        0x701F => Some("QK_MAGIC_EE_HANDS_RIGHT"),
        0x7020 => Some("QK_MAGIC_SWAP_ESCAPE_CAPS_LOCK"),
        0x7021 => Some("QK_MAGIC_UNSWAP_ESCAPE_CAPS_LOCK"),
        0x7022 => Some("QK_MAGIC_TOGGLE_ESCAPE_CAPS_LOCK"),
        _ => None,
    }
}

fn basic_key_name(keycode: u16) -> String {
    match keycode {
        0x00 => "KC_NO".into(),
        0x01 => "KC_TRNS".into(),
        0x02 => "KC_ERROR_ROLL_OVER".into(),
        0x03 => "KC_POST_FAIL".into(),
        0x04..=0x1D => format!("KC_{}", (b'A' + (keycode - 0x04) as u8) as char),
        0x1E..=0x27 => format!("KC_{}", (keycode - 0x1E + 1) % 10),
        0x28 => "KC_ENT".into(),
        0x29 => "KC_ESC".into(),
        0x2A => "KC_BSPC".into(),
        0x2B => "KC_TAB".into(),
        0x2C => "KC_SPC".into(),
        0x2D => "KC_MINS".into(),
        0x2E => "KC_EQL".into(),
        0x2F => "KC_LBRC".into(),
        0x30 => "KC_RBRC".into(),
        0x31 => "KC_BSLS".into(),
        0x33 => "KC_SCLN".into(),
        0x34 => "KC_QUOT".into(),
        0x35 => "KC_GRV".into(),
        0x36 => "KC_COMM".into(),
        0x37 => "KC_DOT".into(),
        0x38 => "KC_SLSH".into(),
        0x39 => "KC_CAPS".into(),
        0x3A..=0x45 => format!("KC_F{}", keycode - 0x39),
        0x46 => "KC_PSCR".into(),
        0x47 => "KC_SCRL".into(),
        0x48 => "KC_PAUS".into(),
        0x49 => "KC_INS".into(),
        0x4A => "KC_HOME".into(),
        0x4B => "KC_PGUP".into(),
        0x4C => "KC_DEL".into(),
        0x4D => "KC_END".into(),
        0x4E => "KC_PGDN".into(),
        0x4F => "KC_RGHT".into(),
        0x50 => "KC_LEFT".into(),
        0x51 => "KC_DOWN".into(),
        0x52 => "KC_UP".into(),
        0x53 => "KC_NLCK".into(),
        0x54 => "KC_PSLH".into(),
        0x55 => "KC_PAST".into(),
        0x56 => "KC_PMNS".into(),
        0x57 => "KC_PPLS".into(),
        0x58 => "KC_PENT".into(),
        0x59 => "KC_P1".into(),
        0x5A => "KC_P2".into(),
        0x5B => "KC_P3".into(),
        0x5C => "KC_P4".into(),
        0x5D => "KC_P5".into(),
        0x5E => "KC_P6".into(),
        0x5F => "KC_P7".into(),
        0x60 => "KC_P8".into(),
        0x61 => "KC_P9".into(),
        0x62 => "KC_P0".into(),
        0x63 => "KC_PDOT".into(),
        0x64 => "KC_NUBS".into(),
        0x68..=0x73 => format!("KC_F{}", keycode - 0x67),
        0x85 => "KC_MINS".into(),
        0xA8 => "KC_MUTE".into(),
        0xA9 => "KC_VOLU".into(),
        0xAA => "KC_VOLD".into(),
        0xAB => "KC_MNXT".into(),
        0xAC => "KC_MPRV".into(),
        0xAD => "KC_MSTP".into(),
        0xAE => "KC_MPLY".into(),
        0xAF => "KC_MSEL".into(),
        0xB0 => "KC_EJCT".into(),
        0xCD => "KC_MS_UP".into(),
        0xCE => "KC_MS_DOWN".into(),
        0xCF => "KC_MS_LEFT".into(),
        0xD0 => "KC_MS_RIGHT".into(),
        0xD1 => "MS_BTN1".into(),
        0xD2 => "MS_BTN2".into(),
        0xD3 => "MS_BTN3".into(),
        0xD4 => "MS_BTN4".into(),
        0xD5 => "MS_BTN5".into(),
        0xD6 => "MS_BTN6".into(),
        0xD7 => "MS_BTN7".into(),
        0xD8 => "MS_BTN8".into(),
        0xD9 => "MS_WHLU".into(),
        0xDA => "MS_WHLD".into(),
        0xDB => "MS_WHLL".into(),
        0xDC => "MS_WHLR".into(),
        0xE0 => "KC_LCTL".into(),
        0xE1 => "KC_LSFT".into(),
        0xE2 => "KC_LALT".into(),
        0xE3 => "KC_LGUI".into(),
        0xE4 => "KC_RCTL".into(),
        0xE5 => "KC_RSFT".into(),
        0xE6 => "KC_RALT".into(),
        0xE7 => "KC_RGUI".into(),
        _ => format!("0x{keycode:02X}"),
    }
}

fn keycode_name(keycode: u16) -> String {
    if keycode <= 0xFF {
        return basic_key_name(keycode);
    }

    if (0x0100..=0x1FFF).contains(&keycode) {
        let mods = (keycode >> 8) & 0x1F;
        let base = keycode & 0xFF;
        if mods == 0x02 {
            if let Some(alias) = us_shift_alias(base) {
                return alias.to_string();
            }
        }
        return wrap_mods(basic_key_name(base), mods);
    }

    if (0x2000..=0x3FFF).contains(&keycode) {
        let mods = (keycode >> 8) & 0x1F;
        let tap = keycode & 0xFF;
        if let Some(alias) = single_mod_alias(mods) {
            return format!("{alias}_T({})", keycode_name(tap));
        }
        if mods == 0x07 || mods == 0x0F || mods == 0x1F {
            return format!("{}_T({})", format_mods(mods), keycode_name(tap));
        }
        return format!("MT({}, {})", format_mods(mods), keycode_name(tap));
    }

    if (0x4000..=0x4FFF).contains(&keycode) {
        let layer = (keycode >> 8) & 0x0F;
        let tap = keycode & 0xFF;
        return format!("LT({layer}, {})", keycode_name(tap));
    }

    if (0x5000..=0x51FF).contains(&keycode) {
        let layer = (keycode >> 5) & 0x0F;
        let mods = keycode & 0x1F;
        return format!("LM({layer}, {})", format_mods(mods));
    }

    if (0x5200..=0x521F).contains(&keycode) {
        return format!("TO({})", keycode & 0x1F);
    }
    if (0x5220..=0x523F).contains(&keycode) {
        return format!("MO({})", keycode & 0x1F);
    }
    if (0x5240..=0x525F).contains(&keycode) {
        return format!("DF({})", keycode & 0x1F);
    }
    if (0x5260..=0x527F).contains(&keycode) {
        return format!("TG({})", keycode & 0x1F);
    }
    if (0x5280..=0x529F).contains(&keycode) {
        return format!("OSL({})", keycode & 0x1F);
    }
    if (0x52A0..=0x52BF).contains(&keycode) {
        return format!("OSM({})", format_mods(keycode & 0x1F));
    }
    if (0x52C0..=0x52DF).contains(&keycode) {
        return format!("TT({})", keycode & 0x1F);
    }
    if (0x52E0..=0x52FF).contains(&keycode) {
        return format!("PDF({})", keycode & 0x1F);
    }

    if (0x5600..=0x56FF).contains(&keycode) {
        let special = match keycode {
            0x56F0 => Some("QK_SWAP_HANDS_TOGGLE"),
            0x56F1 => Some("QK_SWAP_HANDS_TAP_TOGGLE"),
            0x56F2 => Some("QK_SWAP_HANDS_MOMENTARY_ON"),
            0x56F3 => Some("QK_SWAP_HANDS_MOMENTARY_OFF"),
            0x56F4 => Some("QK_SWAP_HANDS_OFF"),
            0x56F5 => Some("QK_SWAP_HANDS_ON"),
            0x56F6 => Some("QK_SWAP_HANDS_ONE_SHOT"),
            _ => None,
        };
        if let Some(name) = special {
            return name.to_string();
        }
        return format!("SH_T({})", keycode_name(keycode & 0xFF));
    }

    if (0x5700..=0x57FF).contains(&keycode) {
        return format!("TD({})", keycode & 0xFF);
    }

    if let Some(name) = special_magic_name(keycode) {
        return name.to_string();
    }

    if keycode == 0x7100 {
        return "QK_MIDI_ON".into();
    }
    if keycode == 0x7101 {
        return "QK_MIDI_OFF".into();
    }
    if keycode == 0x7102 {
        return "QK_MIDI_TOGGLE".into();
    }
    if (0x7103..=0x714A).contains(&keycode) {
        let midi_notes = [
            "C", "C_SHARP", "D", "D_SHARP", "E", "F", "F_SHARP", "G", "G_SHARP", "A", "A_SHARP",
            "B",
        ];
        let idx = (keycode - 0x7103) as usize;
        let note = midi_notes[idx % 12];
        let octave = idx / 12;
        return format!("QK_MIDI_NOTE_{note}_{octave}");
    }
    if (0x714B..=0x7154).contains(&keycode) {
        let suffixes = ["N2", "N1", "0", "1", "2", "3", "4", "5", "6", "7"];
        return format!("QK_MIDI_OCTAVE_{}", suffixes[(keycode - 0x714B) as usize]);
    }
    if keycode == 0x7155 {
        return "QK_MIDI_OCTAVE_DOWN".into();
    }
    if keycode == 0x7156 {
        return "QK_MIDI_OCTAVE_UP".into();
    }
    if (0x7157..=0x7163).contains(&keycode) {
        let suffixes = [
            "N6", "N5", "N4", "N3", "N2", "N1", "0", "1", "2", "3", "4", "5", "6",
        ];
        return format!(
            "QK_MIDI_TRANSPOSE_{}",
            suffixes[(keycode - 0x7157) as usize]
        );
    }
    if keycode == 0x7164 {
        return "QK_MIDI_TRANSPOSE_DOWN".into();
    }
    if keycode == 0x7165 {
        return "QK_MIDI_TRANSPOSE_UP".into();
    }
    if (0x7166..=0x7170).contains(&keycode) {
        return format!("QK_MIDI_VELOCITY_{}", keycode - 0x7166);
    }
    if keycode == 0x7171 {
        return "QK_MIDI_VELOCITY_DOWN".into();
    }
    if keycode == 0x7172 {
        return "QK_MIDI_VELOCITY_UP".into();
    }
    if (0x7173..=0x7182).contains(&keycode) {
        return format!("QK_MIDI_CHANNEL_{}", keycode - 0x7173 + 1);
    }

    if (0x7200..=0x73FF).contains(&keycode) {
        return format!("QK_SEQUENCER+{}", keycode - 0x7200);
    }
    if (0x7480..=0x74BF).contains(&keycode) {
        return format!("QK_AUDIO+{}", keycode - 0x7480);
    }
    if (0x74C0..=0x74FF).contains(&keycode) {
        return format!("QK_STENO+{}", keycode - 0x74C0);
    }
    if (0x7780..=0x77BF).contains(&keycode) {
        return format!("QK_CONNECTION+{}", keycode - 0x7780);
    }
    if (0x7800..=0x78FF).contains(&keycode) {
        return format!("QK_LIGHTING+{}", keycode - 0x7800);
    }
    if (0x7C00..=0x7DFF).contains(&keycode) {
        if keycode == 0x7C00 {
            return "QK_BOOT".into();
        }
        return format!("QK_QUANTUM+{}", keycode - 0x7C00);
    }

    format!("0x{keycode:04X}")
}

fn rotated_box_bounds(box_item: &KeyBox) -> (f32, f32, f32, f32) {
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
