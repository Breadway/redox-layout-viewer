use eframe::egui::Color32;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KeyboardDefinition {
    pub name: Option<String>,
    #[serde(
        rename = "vendorId",
        default,
        deserialize_with = "deserialize_optional_u16_flexible"
    )]
    pub vendor_id: Option<u16>,
    #[serde(
        rename = "productId",
        default,
        deserialize_with = "deserialize_optional_u16_flexible"
    )]
    pub product_id: Option<u16>,
    pub matrix: MatrixDefinition,
    pub layouts: LayoutsDefinition,
}

pub fn deserialize_optional_u16_flexible<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MatrixDefinition {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LayoutsDefinition {
    pub keymap: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub keyboard_id: u64,
    pub vial_protocol: u32,
    pub via_protocol: u16,
    pub layout_options: u32,
    pub definition: Arc<KeyboardDefinition>,
    pub keymap: Vec<Vec<Vec<u16>>>,
    pub layer_state: LayerState,
    pub keyboxes: Vec<KeyBox>,
}

#[derive(Debug, Clone, Default)]
pub struct LayerState {
    pub effective_layer: usize,
    pub active_layer: usize,
    pub default_layer: usize,
    pub layer_state: u32,
    pub default_layer_state: u32,
}

#[derive(Debug, Clone)]
pub struct KeyBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
    pub rx: f32,
    pub ry: f32,
    pub matrix: String,
}

#[derive(Debug, Clone)]
pub struct RenderBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rotation: f32,
    pub rx: f32,
    pub ry: f32,
    pub fill: Color32,
    pub keybind: String,
    pub transparent: bool,
}

#[derive(Debug, Clone)]
pub struct KeyEdge {
    pub bind: String,
    pub pressed: bool,
}
