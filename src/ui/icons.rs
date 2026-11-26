use eframe::egui::{self, Pos2, Rect, Response, Shape, Stroke, Vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    Add,
    Stack,
    Fit,
    Remove,
    RightAxis,
    Left,
    Right,
    Play,
    Pause,
    Moon,
    Sun,
}

/// Draw a small vector icon so controls look the same with bundled fonts,
/// installed fonts, and in the browser where emoji fonts are not available.
pub fn button(ui: &mut egui::Ui, icon: Icon, tooltip: &str, selected: bool) -> Response {
    let response = ui.add_sized(
        Vec2::new(28.0, 24.0),
        egui::Button::new("").selected(selected),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    let rect = response.rect.shrink(6.0);
    let color = ui.visuals().text_color();
    let stroke = Stroke::new(1.7, color);
    let center = rect.center();
    let painter = ui.painter();
    match icon {
        Icon::Add => {
            painter.line_segment(
                [
                    Pos2::new(center.x, rect.top()),
                    Pos2::new(center.x, rect.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(rect.left(), center.y),
                    Pos2::new(rect.right(), center.y),
                ],
                stroke,
            );
        }
        Icon::Remove => {
            painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
            painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
        }
        Icon::Left | Icon::Right => {
            let sign = if matches!(icon, Icon::Left) {
                -1.0
            } else {
                1.0
            };
            let tip = Pos2::new(center.x + sign * rect.width() * 0.4, center.y);
            let tail = Pos2::new(center.x - sign * rect.width() * 0.35, center.y);
            painter.line_segment([tail, tip], stroke);
            painter.line_segment([tip, Pos2::new(tip.x - sign * 4.0, tip.y - 4.0)], stroke);
            painter.line_segment([tip, Pos2::new(tip.x - sign * 4.0, tip.y + 4.0)], stroke);
        }
        Icon::Stack => {
            let back = rect.translate(Vec2::new(-2.0, -2.0));
            painter.rect_stroke(back, 1.0, stroke, egui::StrokeKind::Inside);
            painter.rect_stroke(
                rect.shrink2(Vec2::new(2.0, 2.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Icon::Fit => {
            let x = rect.width() * 0.28;
            let y = rect.height() * 0.32;
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let corner = center + Vec2::new(sx * x, sy * y);
                painter.line_segment([corner, corner + Vec2::new(-sx * 4.0, 0.0)], stroke);
                painter.line_segment([corner, corner + Vec2::new(0.0, -sy * 4.0)], stroke);
            }
        }
        Icon::RightAxis => {
            let x = rect.right() - 1.0;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(rect.left() + 1.0, center.y),
                    Pos2::new(x - 1.0, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(x - 1.0, center.y),
                    Pos2::new(x - 5.0, center.y - 3.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(x - 1.0, center.y),
                    Pos2::new(x - 5.0, center.y + 3.5),
                ],
                stroke,
            );
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                vec![
                    rect.left_top(),
                    Pos2::new(rect.right(), center.y),
                    rect.left_bottom(),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            let bar = rect.width() * 0.28;
            painter.rect_filled(
                Rect::from_min_max(rect.left_top(), Pos2::new(rect.left() + bar, rect.bottom())),
                0.0,
                color,
            );
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(rect.right() - bar, rect.top()),
                    rect.right_bottom(),
                ),
                0.0,
                color,
            );
        }
        Icon::Sun => {
            painter.circle_filled(center, 4.0, color);
            for i in 0..8 {
                let direction = Vec2::angled(i as f32 * std::f32::consts::TAU / 8.0);
                painter.line_segment([center + direction * 6.0, center + direction * 9.0], stroke);
            }
        }
        Icon::Moon => {
            painter.circle_filled(center, 8.0, color);
            painter.circle_filled(
                center + Vec2::new(4.0, -3.0),
                7.0,
                ui.visuals().widgets.inactive.bg_fill,
            );
        }
    }
    response.on_hover_text(tooltip)
}
