//! Offline renderer: rasterises every keymap layer to a full-HD PNG without
//! opening a window. Shares colour/label/transparency logic with the live UI
//! but paints with tiny-skia (shapes), resvg (icons) and ab_glyph (text)
//! instead of egui, so it runs headless.

use std::path::Path;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use anyhow::{anyhow, bail, Context, Result};
use eframe::egui::Color32;
use resvg::tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Shader, Stroke, Transform,
};

use crate::icons::icon_svg_bytes;
use crate::keycode::{active_label, keycode_name};
use crate::layout::{build_fixed_color_overrides, parse_matrix_ref, rotated_box_bounds};
use crate::render::luminance;
use crate::types::Snapshot;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const PAD: f32 = 44.0;
const KC_TRNS: u16 = 0x0001;

/// Renders one PNG per layer (`layer_0.png`, `layer_1.png`, …) into `out_dir`.
pub fn render_layers(snapshot: &Snapshot, out_dir: &Path) -> Result<()> {
    if snapshot.keyboxes.is_empty() || snapshot.keymap.is_empty() {
        bail!("snapshot has no layout to render");
    }
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let font = load_font()?;
    let fixed_color_overrides = build_fixed_color_overrides(snapshot);

    // Layout transform is layer-independent: derive it once.
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for b in &snapshot.keyboxes {
        let (bx0, by0, bx1, by1) = rotated_box_bounds(b);
        min_x = min_x.min(bx0);
        min_y = min_y.min(by0);
        max_x = max_x.max(bx1);
        max_y = max_y.max(by1);
    }
    let anchors: Vec<_> = snapshot.keyboxes.iter().filter(|b| b.r == 0.0).collect();
    let (ax_min, ay_min, ax_max, ay_max) = if anchors.is_empty() {
        (min_x, min_y, max_x, max_y)
    } else {
        (
            anchors.iter().map(|b| b.x).fold(f32::INFINITY, f32::min),
            anchors.iter().map(|b| b.y).fold(f32::INFINITY, f32::min),
            anchors
                .iter()
                .map(|b| b.x + b.w)
                .fold(f32::NEG_INFINITY, f32::max),
            anchors
                .iter()
                .map(|b| b.y + b.h)
                .fold(f32::NEG_INFINITY, f32::max),
        )
    };

    let scale_x = (WIDTH as f32 - PAD * 2.0) / (max_x - min_x).max(1.0);
    let scale_y = (HEIGHT as f32 - PAD * 2.0) / (max_y - min_y).max(1.0);
    let scale = scale_x.min(scale_y).max(0.15);
    let anchor_cx = (ax_min + ax_max) * 0.5;
    let anchor_cy = (ay_min + ay_max) * 0.5;
    let ox = WIDTH as f32 * 0.5 - anchor_cx * scale;
    let oy = HEIGHT as f32 * 0.5 - (anchor_cy + 0.12) * scale;

    for layer in 0..snapshot.keymap.len() {
        let mut pixmap =
            Pixmap::new(WIDTH, HEIGHT).context("failed to allocate framebuffer")?;
        pixmap.fill(Color::from_rgba8(255, 255, 255, 255));

        for box_item in &snapshot.keyboxes {
            let (row, col) = parse_matrix_ref(&box_item.matrix);
            let keycode = resolve_keycode(snapshot, row, col, layer);
            let transparent = is_transparent(snapshot, row, col, layer);
            let keybind = keycode_name(keycode);
            let fill = fixed_color_overrides
                .get(&(row, col))
                .copied()
                .unwrap_or(Color32::from_rgb(80, 80, 80));

            draw_key(
                &mut pixmap,
                box_item,
                scale,
                ox,
                oy,
                fill,
                transparent,
                &keybind,
                &font,
            );
        }

        let path = out_dir.join(format!("layer_{layer}.png"));
        pixmap
            .save_png(&path)
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!("[imgrender] wrote {}", path.display());
    }

    Ok(())
}

/// Same fall-through resolution as the live app: scan active layers (this
/// layer + base 0) top-down, KC_TRNS passes through, KC_NO is opaque.
fn resolve_keycode(snapshot: &Snapshot, row: usize, col: usize, layer: usize) -> u16 {
    let keymap = &snapshot.keymap;
    for l in (0..keymap.len()).rev() {
        if l != layer && l != 0 {
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
        .first()
        .and_then(|rows| rows.get(row))
        .and_then(|cols| cols.get(col))
        .copied()
        .unwrap_or(0)
}

fn is_transparent(snapshot: &Snapshot, row: usize, col: usize, layer: usize) -> bool {
    let keymap = &snapshot.keymap;
    for l in (0..keymap.len()).rev() {
        if l != layer && l != 0 {
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

#[allow(clippy::too_many_arguments)]
fn draw_key(
    pixmap: &mut Pixmap,
    box_item: &crate::types::KeyBox,
    scale: f32,
    ox: f32,
    oy: f32,
    fill: Color32,
    transparent: bool,
    keybind: &str,
    font: &FontVec,
) {
    let kw = (box_item.w * scale).max(1.0);
    let kh = (box_item.h * scale).max(1.0);
    let tw = kw.ceil() as u32;
    let th = kh.ceil() as u32;

    let Some(mut key) = Pixmap::new(tw.max(1), th.max(1)) else {
        return;
    };

    let radius = (kw.min(kh) * 0.14).max(2.0);
    let path = match rounded_rect(kw, kh, radius) {
        Some(p) => p,
        None => return,
    };

    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.shader = Shader::SolidColor(Color::from_rgba8(fill.r(), fill.g(), fill.b(), 255));
    key.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);

    let mut stroke_paint = Paint::default();
    stroke_paint.anti_alias = true;
    stroke_paint.shader = Shader::SolidColor(Color::from_rgba8(54, 56, 61, 255));
    key.stroke_path(
        &path,
        &stroke_paint,
        &Stroke {
            width: 1.0,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );

    let text_color = if transparent {
        (0, 0, 0)
    } else if luminance(fill) < 0.55 {
        (247, 247, 250)
    } else {
        (36, 36, 36)
    };

    let icon_size = (kw.min(kh) * 0.82) as u32;
    if let Some(svg) = icon_svg_bytes(keybind) {
        if icon_size > 0 {
            if let Some(icon) = render_icon(svg, icon_size, text_color) {
                let dx = ((kw - icon_size as f32) * 0.5).round() as i32;
                let dy = ((kh - icon_size as f32) * 0.5).round() as i32;
                key.draw_pixmap(
                    dx,
                    dy,
                    icon.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
    } else {
        let label = active_label(keybind, false);
        if !label.is_empty() {
            draw_label(&mut key, font, &label, kw, kh, text_color);
        }
    }

    // Composite the key onto the framebuffer, rotating about (rx, ry) if set.
    let kx = ox + box_item.x * scale;
    let ky = oy + box_item.y * scale;
    if box_item.r.abs() > f32::EPSILON {
        let orx = ox + box_item.rx * scale;
        let ory = oy + box_item.ry * scale;
        let transform = Transform::from_translate(orx, ory)
            .pre_rotate(box_item.r)
            .pre_translate(-orx, -ory)
            .pre_translate(kx, ky);
        pixmap.draw_pixmap(
            0,
            0,
            key.as_ref(),
            &PixmapPaint::default(),
            transform,
            None,
        );
    } else {
        pixmap.draw_pixmap(
            kx.round() as i32,
            ky.round() as i32,
            key.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
}

fn rounded_rect(w: f32, h: f32, r: f32) -> Option<resvg::tiny_skia::Path> {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(r, 0.0);
    pb.line_to(w - r, 0.0);
    pb.quad_to(w, 0.0, w, r);
    pb.line_to(w, h - r);
    pb.quad_to(w, h, w - r, h);
    pb.line_to(r, h);
    pb.quad_to(0.0, h, 0.0, h - r);
    pb.line_to(0.0, r);
    pb.quad_to(0.0, 0.0, r, 0.0);
    pb.close();
    pb.finish()
}

fn render_icon(svg: &[u8], size: u32, color: (u8, u8, u8)) -> Option<Pixmap> {
    let hex = format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2);
    let src = std::str::from_utf8(svg).ok()?.replace("currentColor", &hex);
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(&src, &opt).ok()?;
    let sx = size as f32 / tree.size().width();
    let sy = size as f32 / tree.size().height();
    let mut pm = Pixmap::new(size, size)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pm.as_mut(),
    );
    Some(pm)
}

/// Picks a fitting font size, centres the (possibly multi-line) label and
/// composites each glyph with premultiplied source-over.
fn draw_label(
    key: &mut Pixmap,
    font: &FontVec,
    label: &str,
    kw: f32,
    kh: f32,
    color: (u8, u8, u8),
) {
    let lines: Vec<&str> = label.lines().collect();
    let line_count = lines.len().max(1) as f32;

    let mut size = if label.chars().count() <= 3 {
        kh * 0.42
    } else if label.chars().count() > 10 {
        kh * 0.26
    } else {
        kh * 0.34
    };
    let min_size = 6.0;

    let measure = |px: f32| -> f32 {
        let sf = font.as_scaled(PxScale::from(px));
        lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|c| sf.h_advance(font.glyph_id(c)))
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max)
    };

    let max_w = kw * 0.86;
    let widest = measure(size);
    if widest > max_w && widest > 0.0 {
        size = (size * (max_w / widest)).max(min_size);
    }
    let max_h = kh * 0.86;
    let total_h = line_count * size * 1.18;
    if total_h > max_h && total_h > 0.0 {
        size = (size * (max_h / total_h)).max(min_size);
    }

    let sf = font.as_scaled(PxScale::from(size));
    let line_h = size * 1.18;
    let block_h = line_count * line_h;
    let mut baseline = (kh - block_h) * 0.5 + sf.ascent();

    for line in &lines {
        let line_w: f32 = line
            .chars()
            .map(|c| sf.h_advance(font.glyph_id(c)))
            .sum();
        let mut pen_x = (kw - line_w) * 0.5;
        for ch in line.chars() {
            let gid = font.glyph_id(ch);
            let glyph = gid.with_scale_and_position(
                PxScale::from(size),
                ab_glyph::point(pen_x, baseline),
            );
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    if px < 0
                        || py < 0
                        || px >= key.width() as i32
                        || py >= key.height() as i32
                    {
                        return;
                    }
                    blend_px(key, px as u32, py as u32, color, coverage);
                });
            }
            pen_x += sf.h_advance(gid);
        }
        baseline += line_h;
    }
}

/// Premultiplied source-over of a solid colour at `coverage` alpha.
fn blend_px(pixmap: &mut Pixmap, x: u32, y: u32, color: (u8, u8, u8), coverage: f32) {
    let a = coverage.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let w = pixmap.width();
    let idx = ((y * w + x) * 4) as usize;
    let data = pixmap.data_mut();
    let inv = 1.0 - a;
    let sr = color.0 as f32 * a;
    let sg = color.1 as f32 * a;
    let sb = color.2 as f32 * a;
    data[idx] = (sr + data[idx] as f32 * inv).round().clamp(0.0, 255.0) as u8;
    data[idx + 1] = (sg + data[idx + 1] as f32 * inv).round().clamp(0.0, 255.0) as u8;
    data[idx + 2] = (sb + data[idx + 2] as f32 * inv).round().clamp(0.0, 255.0) as u8;
    data[idx + 3] =
        (a * 255.0 + data[idx + 3] as f32 * inv).round().clamp(0.0, 255.0) as u8;
}

fn load_font() -> Result<FontVec> {
    let path = std::process::Command::new("fc-match")
        .args(["--format=%{file}", "sans:style=Regular"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .context("could not resolve a system font via fc-match")?;
    let bytes = std::fs::read(&path)
        .with_context(|| format!("failed to read font {path}"))?;
    FontVec::try_from_vec(bytes).map_err(|e| anyhow!("failed to parse font {path}: {e}"))
}
