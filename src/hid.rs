use anyhow::{anyhow, Context, Result};
use crossbeam_channel::Sender;
use eframe::egui;
use hidapi::{HidApi, HidDevice};
use std::io::Read;
use std::sync::Arc;
use std::thread;
use xz2::read::XzDecoder;

use crate::keycode::keycode_name;
use crate::layout::iter_keyboxes;
use crate::types::{KeyboardDefinition, KeyEdge, LayerState, Snapshot};

const RAW_USAGE_PAGE: u16 = 0xFF60;
const RAW_USAGE: u16 = 0x61;
const KBD_USAGE_PAGE: u16 = 0x0001;
const KBD_USAGE: u16 = 0x0006;
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

pub fn find_raw_device(api: &HidApi, vid: u16, pid: u16) -> Result<hidapi::DeviceInfo> {
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

pub fn find_keyboard_hid_device(api: &HidApi, vid: u16, pid: u16) -> Option<hidapi::DeviceInfo> {
    // Primary: find by HID usage page / usage
    api.device_list()
        .find(|d| {
            d.vendor_id() == vid
                && d.product_id() == pid
                && d.usage_page() == KBD_USAGE_PAGE
                && d.usage() == KBD_USAGE
        })
        .cloned()
        .or_else(|| {
            // Fallback: first non-raw interface (interface 0 is usually the keyboard)
            api.device_list()
                .find(|d| {
                    d.vendor_id() == vid
                        && d.product_id() == pid
                        && d.usage_page() != RAW_USAGE_PAGE
                        && d.interface_number() == 0
                })
                .cloned()
        })
}

/// Reads HID keyboard reports directly from the hidraw interface.
///
/// This bypasses libinput's evdev grab (which silences evdev readers on Wayland)
/// because hidraw is a separate kernel interface that is never exclusively grabbed.
pub fn spawn_kbd_hid_reader(
    device: HidDevice,
    tx: Sender<KeyEdge>,
    ctx: egui::Context,
) -> Result<()> {
    thread::Builder::new()
        .name("kbd-hid-reader".into())
        .spawn(move || {
            let mut prev_mods = 0u8;
            let mut prev_keys = [0u8; 6];
            let mut buf = [0u8; 64];

            loop {
                match device.read(&mut buf) {
                    Ok(0) => continue,
                    Ok(n) => {
                        // Detect whether the report uses a report ID prefix.
                        // With report ID 0x01: [0x01, mods, reserved, k0..k5] = 9 bytes
                        // Boot protocol / no IDs: [mods, reserved, k0..k5] = 8 bytes
                        let (mods, keys): (u8, [u8; 6]) = if n == 8 {
                            (buf[0], buf[2..8].try_into().unwrap())
                        } else if n >= 9 && buf[0] == 0x01 {
                            (buf[1], buf[3..9].try_into().unwrap())
                        } else {
                            continue;
                        };

                        let mut changed = false;

                        if mods != prev_mods {
                            const MOD_BITS: [(u8, u16); 8] = [
                                (0x01, 0xE0), // KC_LCTL
                                (0x02, 0xE1), // KC_LSFT
                                (0x04, 0xE2), // KC_LALT
                                (0x08, 0xE3), // KC_LGUI
                                (0x10, 0xE4), // KC_RCTL
                                (0x20, 0xE5), // KC_RSFT
                                (0x40, 0xE6), // KC_RALT
                                (0x80, 0xE7), // KC_RGUI
                            ];
                            for (bit, hid_code) in MOD_BITS {
                                let was = prev_mods & bit != 0;
                                let now = mods & bit != 0;
                                if was != now {
                                    let _ = tx.send(KeyEdge {
                                        bind: keycode_name(hid_code),
                                        pressed: now,
                                    });
                                    changed = true;
                                }
                            }
                            prev_mods = mods;
                        }

                        for &k in &keys {
                            if k != 0 && !prev_keys.contains(&k) {
                                let _ = tx.send(KeyEdge {
                                    bind: keycode_name(k as u16),
                                    pressed: true,
                                });
                                changed = true;
                            }
                        }
                        for &k in &prev_keys {
                            if k != 0 && !keys.contains(&k) {
                                let _ = tx.send(KeyEdge {
                                    bind: keycode_name(k as u16),
                                    pressed: false,
                                });
                                changed = true;
                            }
                        }
                        prev_keys = keys;

                        if changed {
                            ctx.request_repaint();
                        }
                    }
                    Err(e) => {
                        eprintln!("[hid-kbd] read error: {e}");
                        break;
                    }
                }
            }
        })
        .context("failed to spawn kbd-hid reader")?;
    Ok(())
}

pub fn spawn_layer_reader(
    device: HidDevice,
    tx: Sender<LayerState>,
    ctx: egui::Context,
) -> Result<()> {
    thread::Builder::new()
        .name("layer-reader".into())
        .spawn(move || loop {
            let mut buf = [0u8; RAW_REPORT_LEN];
            match device.read(&mut buf) {
                Ok(0) => continue,
                Ok(_) => {
                    if let Some(report) = parse_layer_report(&buf) {
                        let _ = tx.send(report);
                        ctx.request_repaint();
                    }
                }
                Err(_) => break,
            }
        })
        .context("failed to spawn layer reader thread")?;
    Ok(())
}

pub fn load_snapshot(device: &HidDevice) -> Result<Snapshot> {
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

    Ok(Snapshot {
        keyboard_id,
        vial_protocol,
        via_protocol,
        layout_options,
        definition,
        keymap,
        layer_state: LayerState::default(),
        keyboxes,
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
