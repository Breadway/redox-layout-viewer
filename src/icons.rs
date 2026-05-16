use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::HashMap;

const ICON_SIZE: u32 = 256;

const ICON_ASSETS: &[(&str, &[u8])] = &[
    ("MO(1)", include_bytes!("../assets/MO(1).svg")),
    ("MO(2)", include_bytes!("../assets/MO(2).svg")),
    ("MO(3)", include_bytes!("../assets/MO(3).svg")),
    ("TG(4)", include_bytes!("../assets/TG(4).svg")),
    ("LGUI(KC_1)", include_bytes!("../assets/LGUI(KC_1).svg")),
    ("LGUI(KC_2)", include_bytes!("../assets/LGUI(KC_2).svg")),
    ("LGUI(KC_3)", include_bytes!("../assets/LGUI(KC_3).svg")),
    ("LGUI(KC_4)", include_bytes!("../assets/LGUI(KC_4).svg")),
    ("LGUI(KC_5)", include_bytes!("../assets/LGUI(KC_5).svg")),
    ("LGUI(KC_6)", include_bytes!("../assets/LGUI(KC_6).svg")),
    ("KC_LALT", include_bytes!("../assets/Alt.svg")),
    ("KC_RALT", include_bytes!("../assets/Alt.svg")),
    ("KC_BSPC", include_bytes!("../assets/Backspace.svg")),
    ("KC_LCTL", include_bytes!("../assets/Control.svg")),
    ("KC_RCTL", include_bytes!("../assets/Control.svg")),
    ("KC_DEL", include_bytes!("../assets/Delete.svg")),
    ("KC_DOWN", include_bytes!("../assets/DownArrow.svg")),
    ("KC_END", include_bytes!("../assets/End.svg")),
    ("KC_ENT", include_bytes!("../assets/Enter.svg")),
    ("KC_ESC", include_bytes!("../assets/Escape.svg")),
    ("KC_HOME", include_bytes!("../assets/Home.svg")),
    ("KC_INS", include_bytes!("../assets/Insert.svg")),
    ("KC_LEFT", include_bytes!("../assets/LeftArrow.svg")),
    ("KC_BTN1", include_bytes!("../assets/Mouse1.svg")),
    ("KC_BTN2", include_bytes!("../assets/Mouse2.svg")),
    ("KC_PAUS", include_bytes!("../assets/Pause.svg")),
    ("KC_STOP", include_bytes!("../assets/Pause.svg")),
    ("KC_MSTP", include_bytes!("../assets/Pause.svg")),
    ("KC_WSTP", include_bytes!("../assets/Pause.svg")),
    ("KC_MPLY", include_bytes!("../assets/Play.svg")),
    ("KC_PGUP", include_bytes!("../assets/PageUp.svg")),
    ("KC_PGDN", include_bytes!("../assets/PageDown.svg")),
    ("KC_PSCR", include_bytes!("../assets/PrintScreen.svg")),
    ("KC_RGHT", include_bytes!("../assets/RightArrow.svg")),
    ("KC_LSFT", include_bytes!("../assets/Shift.svg")),
    ("KC_RSFT", include_bytes!("../assets/Shift.svg")),
    ("KC_SPC", include_bytes!("../assets/Space.svg")),
    ("KC_LGUI", include_bytes!("../assets/Super.svg")),
    ("KC_RGUI", include_bytes!("../assets/Super.svg")),
    ("KC_TAB", include_bytes!("../assets/Tab.svg")),
    ("KC_UP", include_bytes!("../assets/UpArrow.svg")),
    ("KC_VOLD", include_bytes!("../assets/VolumeDown.svg")),
    ("KC_VOLU", include_bytes!("../assets/VolumeUp.svg")),
];

pub struct Icons {
    map: HashMap<&'static str, TextureHandle>,
}

impl Icons {
    pub fn load(ctx: &egui::Context) -> Self {
        let mut map = HashMap::new();
        for (bind, svg_bytes) in ICON_ASSETS {
            if let Some(handle) = rasterize(ctx, bind, svg_bytes) {
                map.insert(*bind, handle);
            }
        }
        Self { map }
    }

    pub fn get(&self, bind: &str) -> Option<egui::TextureId> {
        self.map.get(bind).map(|h| h.id())
    }
}

/// Raw SVG bytes for a bind name, for reuse by the offline image renderer.
pub fn icon_svg_bytes(bind: &str) -> Option<&'static [u8]> {
    ICON_ASSETS
        .iter()
        .find(|(name, _)| *name == bind)
        .map(|(_, bytes)| *bytes)
}

fn rasterize(ctx: &egui::Context, name: &str, svg_bytes: &[u8]) -> Option<TextureHandle> {
    let src = std::str::from_utf8(svg_bytes)
        .ok()?
        .replace("currentColor", "white");

    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&src, &opt).ok()?;

    let sx = ICON_SIZE as f32 / tree.size().width();
    let sy = ICON_SIZE as f32 / tree.size().height();
    let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE)?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // premultiplied → straight alpha
    let mut rgba = pixmap.take();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3];
        if a > 0 && a < 255 {
            let inv = 255.0 / a as f32;
            px[0] = (px[0] as f32 * inv).min(255.0) as u8;
            px[1] = (px[1] as f32 * inv).min(255.0) as u8;
            px[2] = (px[2] as f32 * inv).min(255.0) as u8;
        }
    }

    let image =
        ColorImage::from_rgba_unmultiplied([ICON_SIZE as usize, ICON_SIZE as usize], &rgba);
    Some(ctx.load_texture(format!("icon_{name}"), image, TextureOptions::LINEAR))
}
