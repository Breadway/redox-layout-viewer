use eframe::egui::{self, epaint::TextShape, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};

use crate::types::RenderBox;

pub fn draw_box(
    painter: &egui::Painter,
    key_box: &RenderBox,
    scale: f32,
    ox: f32,
    oy: f32,
    highlighted: bool,
    icon: Option<egui::TextureId>,
    label: &str,
) {
    let fill = if highlighted { Color32::WHITE } else { key_box.fill };
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

    let text_color = if key_box.transparent {
        Color32::BLACK
    } else if luminance(fill) < 0.55 {
        Color32::from_rgb(247, 247, 250)
    } else {
        Color32::from_rgb(36, 36, 36)
    };

    if let Some(tex_id) = icon {
        let icon_size = rect.width().min(rect.height()) * 0.82;
        let half = icon_size / 2.0;
        let center = rect.center();
        let corners = [
            Pos2::new(center.x - half, center.y - half),
            Pos2::new(center.x + half, center.y - half),
            Pos2::new(center.x + half, center.y + half),
            Pos2::new(center.x - half, center.y + half),
        ];
        let uvs = [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ];
        let mut mesh = egui::epaint::Mesh::with_texture(tex_id);
        for (pos, uv) in corners.iter().zip(uvs.iter()) {
            let p = if has_rotation { rotate_point(*pos, origin, angle) } else { *pos };
            mesh.vertices.push(egui::epaint::Vertex { pos: p, uv: *uv, color: text_color });
        }
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        painter.add(Shape::Mesh(mesh));
        return;
    }

    if !label.is_empty() {
        let lines: Vec<&str> = label.lines().collect();
        let max_lines = lines.len().max(1) as f32;
        let mut font_size = if label.len() <= 3 {
            16.0
        } else if label.len() > 10 {
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
            let point =
                Pos2::new(center.x + radius * theta.cos(), center.y + radius * theta.sin());
            points.push(rotate_point(point, origin, angle));
        }
    }

    points
}

pub fn rotate_point(point: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let sin = angle.sin();
    let cos = angle.cos();
    let dx = point.x - origin.x;
    let dy = point.y - origin.y;
    Pos2::new(
        origin.x + dx * cos - dy * sin,
        origin.y + dx * sin + dy * cos,
    )
}

pub fn luminance(color: Color32) -> f32 {
    (0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32) / 255.0
}
