use crate::log::{Dataset, FieldKind};
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gauge {
    pub name: String,
    pub unit: String,
    pub occurrence: usize,
    pub min: f64,
    pub max: f64,
    pub nominal_low: Option<f64>,
    pub nominal_high: Option<f64>,
    pub warning_low: Option<f64>,
    pub warning_high: Option<f64>,
    pub critical_low: Option<f64>,
    pub critical_high: Option<f64>,
    pub precision: usize,
    pub size: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Missing,
    Neutral,
    Nominal,
    Warning,
    Critical,
}
impl Gauge {
    pub fn from_field(data: &Dataset, id: usize) -> Option<Self> {
        let f = data.fields.get(id)?;
        if !matches!(f.kind, FieldKind::Numeric { .. }) {
            return None;
        }
        let occurrence = data
            .field_ids(&f.name)
            .iter()
            .filter(|&&other| {
                other < id
                    && data.fields[other].unit == f.unit
                    && matches!(data.fields[other].kind, FieldKind::Numeric { .. })
            })
            .count();
        let (mut min, mut max) = data.statistics(id).map_or((0.0, 1.0), |s| (s.min, s.max));
        if min == max {
            let delta = (min.abs() * 0.1).max(1.0);
            min -= delta;
            max += delta;
        }
        let precision = match f.kind {
            FieldKind::Numeric { digits, .. } => digits.clamp(0, 6) as usize,
            _ => 0,
        };
        let mut g = Self {
            name: f.name.clone(),
            unit: f.unit.clone(),
            occurrence,
            min,
            max,
            nominal_low: None,
            nominal_high: None,
            warning_low: None,
            warning_high: None,
            critical_low: None,
            critical_high: None,
            precision,
            size: 180.0,
        };
        if f.name == "Battery V" && f.unit == "V" {
            g.min = 8.0;
            g.max = 18.0;
            g.nominal_low = Some(12.0);
            g.nominal_high = Some(14.8);
            g.critical_high = Some(16.0);
        }
        Some(g)
    }
    pub fn field_id(&self, data: &Dataset) -> Option<usize> {
        data.field_ids(&self.name)
            .iter()
            .copied()
            .filter(|&id| {
                data.fields[id].unit == self.unit
                    && matches!(data.fields[id].kind, FieldKind::Numeric { .. })
            })
            .nth(self.occurrence)
    }
    pub fn status(&self, value: Option<f64>) -> Status {
        let Some(v) = value.filter(|v| v.is_finite()) else {
            return Status::Missing;
        };
        if self.critical_low.is_some_and(|x| v <= x) || self.critical_high.is_some_and(|x| v >= x) {
            return Status::Critical;
        }
        if self.warning_low.is_some_and(|x| v <= x) || self.warning_high.is_some_and(|x| v >= x) {
            return Status::Warning;
        }
        if let (Some(low), Some(high)) = (self.nominal_low, self.nominal_high) {
            if v < low || v > high {
                return Status::Warning;
            }
            return Status::Nominal;
        }
        Status::Neutral
    }
}
fn limit(ui: &mut egui::Ui, title: &str, value: &mut Option<f64>) {
    ui.horizontal_wrapped(|ui| {
        let mut enabled = value.is_some();
        if ui.checkbox(&mut enabled, title).changed() {
            *value = enabled.then_some(value.unwrap_or(0.0));
        }
        if let Some(v) = value {
            ui.add(egui::DragValue::new(v).speed(0.1));
        }
    });
}
fn point(center: Pos2, radius: f32, angle: f32) -> Pos2 {
    center + Vec2::angled(angle) * radius
}
pub fn draw(
    ui: &mut egui::Ui,
    gauge: &mut Gauge,
    data: &Dataset,
    sample: Option<usize>,
    index: usize,
    max_size: f32,
) -> Action {
    let mut action = Action::None;
    let gauge_size = gauge.size.min(max_size.max(100.0));
    ui.group(|ui| {
        // Keep the header, dial, status, and configuration in one measured card.
        // Previously only the header was inside this group, so the wrapped
        // dashboard measured a short header followed by ungrouped content.
        ui.set_min_width(gauge_size);
        ui.set_max_width(gauge_size);
        ui.vertical(|ui| {
            let label =
                ui.add(egui::Label::new(egui::RichText::new(&gauge.name).strong()).truncate());
            if let Some(id) = gauge.field_id(data) {
                if let crate::log::Provenance::Derived {
                    formula,
                    dependencies,
                } = &data.fields[id].provenance
                {
                    label.on_hover_text(format!(
                        "{}; dependencies: {}",
                        formula,
                        dependencies
                            .iter()
                            .map(|&id| data.fields[id].name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            ui.horizontal_wrapped(|ui| {
                if crate::ui::icons::button(
                    ui,
                    crate::ui::icons::Icon::Left,
                    "Move gauge left",
                    false,
                )
                .clicked()
                {
                    action = Action::Up;
                }
                if crate::ui::icons::button(
                    ui,
                    crate::ui::icons::Icon::Right,
                    "Move gauge right",
                    false,
                )
                .clicked()
                {
                    action = Action::Down;
                }
                if crate::ui::icons::button(
                    ui,
                    crate::ui::icons::Icon::Remove,
                    "Remove this gauge",
                    false,
                )
                .clicked()
                {
                    action = Action::Remove;
                }
            });
            let id = gauge.field_id(data);
            let value =
                id.and_then(|id| sample.and_then(|s| data.columns[id].get(s).copied().flatten()));
            let status = gauge.status(value);
            let color = match status {
                Status::Critical => Color32::LIGHT_RED,
                Status::Warning => Color32::YELLOW,
                Status::Nominal => Color32::LIGHT_GREEN,
                _ => Color32::LIGHT_BLUE,
            };
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(gauge_size), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let center = rect.center() + Vec2::new(0.0, gauge_size * 0.07);
            let radius = gauge_size * 0.40;
            let start = std::f32::consts::PI * 0.75;
            let sweep = std::f32::consts::PI * 1.5;
            let valid = gauge.min.is_finite() && gauge.max.is_finite() && gauge.max > gauge.min;
            for i in 0..48 {
                let t = i as f64 / 48.0;
                let v = gauge.min + (gauge.max - gauge.min) * t;
                let arc_color = if valid {
                    match gauge.status(Some(v)) {
                        Status::Critical => Color32::from_rgb(180, 55, 55),
                        Status::Warning => Color32::from_rgb(180, 150, 40),
                        Status::Nominal => Color32::from_rgb(60, 160, 90),
                        _ => Color32::DARK_GRAY,
                    }
                } else {
                    Color32::DARK_GRAY
                };
                let a = start + sweep * i as f32 / 48.0;
                let b = start + sweep * (i + 1) as f32 / 48.0;
                painter.line_segment(
                    [point(center, radius, a), point(center, radius, b)],
                    Stroke::new(5.0, arc_color),
                );
            }
            if let Some(v) = value.filter(|_| valid) {
                let fraction = ((v - gauge.min) / (gauge.max - gauge.min)).clamp(0.0, 1.0) as f32;
                painter.line_segment(
                    [
                        center,
                        point(center, radius * 0.82, start + sweep * fraction),
                    ],
                    Stroke::new(2.5, color),
                );
                painter.circle_filled(center, 4.0, color);
            }
            let text = match value {
                Some(v) => format!("{:.*} {}", gauge.precision, v, gauge.unit),
                None => "N/A".into(),
            };
            painter.text(
                center + Vec2::new(0.0, gauge_size * 0.19),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(16.0),
                color,
            );
            painter.text(
                rect.left_bottom(),
                egui::Align2::LEFT_BOTTOM,
                format!("{:.1}", gauge.min),
                egui::FontId::proportional(11.0),
                Color32::LIGHT_GRAY,
            );
            painter.text(
                rect.right_bottom(),
                egui::Align2::RIGHT_BOTTOM,
                format!("{:.1}", gauge.max),
                egui::FontId::proportional(11.0),
                Color32::LIGHT_GRAY,
            );
            ui.label(format!(
                "{:?}{}",
                status,
                if value.is_some_and(|v| valid && (v < gauge.min || v > gauge.max)) {
                    " · outside scale"
                } else {
                    ""
                }
            ));
            if !valid {
                ui.colored_label(Color32::YELLOW, "Scale maximum must exceed minimum");
            }
            if id.is_none() {
                ui.label("Field unavailable in this log");
            }
            if let Some(s) = id.and_then(|id| data.statistics(id)) {
                egui::CollapsingHeader::new("Observed statistics")
                    .id_salt(("gauge-stats", index))
                    .show(ui, |ui| {
                        ui.label(format!("Min: {:.3}", s.min));
                        ui.label(format!("Median: {:.3}", s.median));
                        ui.label(format!("Max: {:.3}", s.max));
                        ui.label("Separate from nominal operating limits.");
                    });
            }
            egui::CollapsingHeader::new("Configure")
                .id_salt(("gauge-config", index))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Scale min/max");
                        ui.add(egui::DragValue::new(&mut gauge.min).speed(0.1));
                        ui.add(egui::DragValue::new(&mut gauge.max).speed(0.1));
                    });
                    limit(ui, "Nominal low", &mut gauge.nominal_low);
                    limit(ui, "Nominal high", &mut gauge.nominal_high);
                    limit(ui, "Warning low", &mut gauge.warning_low);
                    limit(ui, "Warning high", &mut gauge.warning_high);
                    limit(ui, "Critical low", &mut gauge.critical_low);
                    limit(ui, "Critical high", &mut gauge.critical_high);
                    ui.add(egui::Slider::new(&mut gauge.precision, 0..=6).text("Precision"));
                    ui.add(egui::Slider::new(&mut gauge.size, 120.0..=320.0).text("Size"));
                });
        });
    });
    action
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Up,
    Down,
    Remove,
}

fn flow_rows(
    ui: &mut egui::Ui,
    widths: &[f32],
    mut draw: impl FnMut(&mut egui::Ui, usize, f32) -> Action,
) -> (Vec<egui::Rect>, Vec<Action>) {
    let available_width = ui.available_width().max(1.0);
    let spacing = ui.spacing().item_spacing.x;
    let widths: Vec<f32> = widths
        .iter()
        .map(|width| width.clamp(1.0, available_width))
        .collect();
    let mut rects = vec![egui::Rect::NOTHING; widths.len()];
    let mut actions = vec![Action::None; widths.len()];
    ui.vertical(|ui| {
        let mut first = 0;
        while first < widths.len() {
            let mut end = first;
            let mut used_width = 0.0;
            while end < widths.len() {
                let next_width = widths[end] + if end == first { 0.0 } else { spacing };
                if end > first && used_width + next_width > available_width {
                    break;
                }
                used_width += next_width;
                end += 1;
            }

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |row_ui| {
                row_ui.spacing_mut().item_spacing.x = spacing;
                for index in first..end {
                    let response = row_ui.allocate_ui_with_layout(
                        Vec2::new(widths[index], 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |card_ui| {
                            card_ui.set_max_width(widths[index]);
                            actions[index] = draw(card_ui, index, widths[index]);
                        },
                    );
                    rects[index] = response.response.rect;
                }
            });
            first = end;
        }
    });
    (rects, actions)
}

pub fn show_wrapped(
    ui: &mut egui::Ui,
    gauges: &mut [Gauge],
    data: &Dataset,
    sample: Option<usize>,
) -> Option<(usize, Action)> {
    ui.spacing_mut().item_spacing = Vec2::splat(8.0);
    let available_width = ui.available_width().max(1.0);
    let widths: Vec<f32> = gauges
        .iter()
        .map(|gauge| (gauge.size + 28.0).max(116.0).min(available_width))
        .collect();
    let (_, actions) = flow_rows(ui, &widths, |card_ui, index, card_width| {
        let max_diameter = (card_width - 28.0).max(60.0);
        draw(
            card_ui,
            &mut gauges[index],
            data,
            sample,
            index,
            max_diameter,
        )
    });
    actions
        .into_iter()
        .enumerate()
        .find(|(_, action)| *action != Action::None)
}

#[cfg(test)]
mod layout_tests {
    use super::flow_rows;
    use eframe::egui::{self, Pos2, Rect, Vec2};

    #[test]
    fn actual_gauge_cards_wrap_to_distinct_rows_without_overflow() {
        let data = crate::log::parse(include_bytes!("../../tests/fixtures/sanitized.mlg")).unwrap();
        let gauge = super::Gauge::from_field(&data, data.field_ids("Battery V")[0]).unwrap();
        let mut gauges = vec![gauge; 3];
        let context = egui::Context::default();
        let mut rects = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0))),
            ..Default::default()
        };
        let _ = context.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(8.0);
                let widths: Vec<_> = gauges.iter().map(|gauge| gauge.size + 28.0).collect();
                rects = flow_rows(ui, &widths, |ui, index, width| {
                    let action = super::draw(
                        ui,
                        &mut gauges[index],
                        &data,
                        None,
                        index,
                        (width - 28.0).max(60.0),
                    );
                    if index == 1 {
                        ui.add_space(90.0);
                    }
                    action
                })
                .0;
            });
        });
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].min.y, rects[1].min.y);
        assert!(rects[2].min.y >= rects[1].max.y);
        assert!(rects[0].right() <= rects[1].left());
        assert_eq!(rects[0].min.x, rects[2].min.x);
    }
}
