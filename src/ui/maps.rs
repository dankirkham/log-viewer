use crate::{
    analysis::maps::{self, Table},
    log::Dataset,
    playback::Playback,
};
use eframe::egui::{self, Color32, FontId, Pos2, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct MapPreferences {
    pub heatmap: bool,
    pub trail_seconds: f64,
    pub ve_choice: usize,
}
impl Default for MapPreferences {
    fn default() -> Self {
        Self {
            heatmap: true,
            trail_seconds: 5.0,
            ve_choice: 0,
        }
    }
}
#[derive(Default)]
pub struct MapState {
    afr: Option<Result<Table, String>>,
    ve: [Option<Result<Table, String>>; 2],
}
#[derive(Clone, Copy)]
struct HeatmapStyle {
    reference: f64,
    invert: bool,
}
impl MapState {
    pub fn load(&mut self, data: &Dataset) {
        self.afr = Some(maps::from_source(data, "afrTable"));
        self.ve = [
            Some(maps::from_source(data, "veTable")),
            Some(maps::from_source(data, "veTable2")),
        ];
    }

    #[cfg(all(target_arch = "wasm32", browser_smoke))]
    pub fn smoke_summary(&self) -> (String, String) {
        let afr = self.afr.as_ref().map_or_else(
            || "not opened".to_owned(),
            |result| {
                result
                    .as_ref()
                    .map_or_else(|error| error.clone(), |_| "available".to_owned())
            },
        );
        let ve = self.ve[0].as_ref().map_or_else(
            || "not opened".to_owned(),
            |result| {
                result
                    .as_ref()
                    .map_or_else(|error| error.clone(), |_| "available".to_owned())
            },
        );
        (afr, ve)
    }
}
pub(crate) fn format_table_number(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-5..3).contains(&exponent) {
        return format!("{value:.2e}");
    }
    let decimals = (2 - exponent).max(0) as usize;
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}
fn format_axis_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format_table_number(value)
    }
}
fn spectrum_color(value: f64, low: f64, high: f64, reference: f64, invert: bool) -> Color32 {
    // The reference value maps to the blue region. This affects color only;
    // the recorded cell is kept.
    let low_color = if invert { 1.0 } else { 0.0 };
    let high_color = if invert { 0.0 } else { 1.0 };
    let x = if value <= reference {
        if reference > low {
            low_color + (0.15 - low_color) * ((value - low) / (reference - low))
        } else {
            0.15
        }
    } else if high > reference {
        0.15 + (high_color - 0.15) * ((value - reference) / (high - reference))
    } else {
        0.15
    }
    .clamp(0.0, 1.0);
    let x2 = x * x;
    let x3 = x2 * x;
    let x4 = x3 * x;
    let x5 = x4 * x;
    let channel = |coefficients: [f64; 6]| {
        (coefficients[0]
            + coefficients[1] * x
            + coefficients[2] * x2
            + coefficients[3] * x3
            + coefficients[4] * x4
            + coefficients[5] * x5)
            .clamp(0.0, 1.0)
    };
    Color32::from_rgb(
        (255.0
            * channel([
                0.13572138,
                4.61539260,
                -42.66032258,
                132.13108234,
                -152.94239396,
                59.28637943,
            ])) as u8,
        (255.0
            * channel([
                0.09140261,
                2.19418839,
                4.84296658,
                -14.18503333,
                4.27729857,
                2.82956604,
            ])) as u8,
        (255.0
            * channel([
                0.10667330,
                12.64194608,
                -60.58204836,
                110.36276771,
                -89.90310912,
                27.34824973,
            ])) as u8,
    )
}
pub(crate) fn afr_color(value: f64, low: f64, high: f64) -> Color32 {
    spectrum_color(value, low, high, 14.7, true)
}
fn draw_table(
    ui: &mut egui::Ui,
    data: &Dataset,
    playback: &Playback,
    table: &Table,
    heatmap: bool,
    seconds: f64,
    style: HeatmapStyle,
) {
    ui.label(format!(
        "{} · RPM by Fuel Load · {}",
        table.name, table.unit
    ));
    let left = 58.0;
    let top = 34.0;
    let nx = table.x.len();
    let ny = table.y.len();
    let w = ((ui.available_width() - left - 8.0) / nx as f32).clamp(18.0, 52.0);
    // Keep table cells readable regardless of the viewport's remaining height.
    // The containing map area scrolls when both tables do not fit vertically.
    let h = (w * 0.72).clamp(14.0, 24.0);
    let size = Vec2::new(left + nx as f32 * w + 6.0, top + ny as f32 * h + 6.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let grid = rect.min + Vec2::new(left, top);
    let min = table.cells.iter().copied().fold(f64::INFINITY, f64::min);
    let max = table
        .cells
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    for (i, x) in table.x.iter().enumerate() {
        painter.text(
            Pos2::new(grid.x + (i as f32 + 0.5) * w, grid.y - 6.0),
            egui::Align2::CENTER_BOTTOM,
            format_axis_number(*x),
            FontId::proportional((w.min(h) * 0.36).clamp(5.5, 8.5)),
            ui.visuals().text_color(),
        );
    }
    for (j, y) in table.y.iter().enumerate() {
        let row = ny - 1 - j;
        painter.text(
            Pos2::new(grid.x - 6.0, grid.y + (row as f32 + 0.5) * h),
            egui::Align2::RIGHT_CENTER,
            format_axis_number(*y),
            FontId::proportional((w.min(h) * 0.36).clamp(5.5, 8.5)),
            ui.visuals().text_color(),
        );
        for i in 0..nx {
            let value = table.cells[j * nx + i];
            let cell = egui::Rect::from_min_size(
                grid + Vec2::new(i as f32 * w, row as f32 * h),
                Vec2::new(w - 1.0, h - 1.0),
            );
            let color = if heatmap {
                if style.invert {
                    afr_color(value, min, max)
                } else {
                    spectrum_color(value, min, max, style.reference, false)
                }
            } else {
                Color32::from_gray(45)
            };
            painter.rect_filled(cell, 0.0, color);
            let luminance =
                0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32;
            painter.text(
                cell.center(),
                egui::Align2::CENTER_CENTER,
                format_table_number(value),
                FontId::proportional((w.min(h) * 0.42).clamp(5.0, 9.5)),
                if luminance > 145.0 {
                    Color32::from_rgb(15, 18, 22)
                } else {
                    Color32::WHITE
                },
            );
            ui.interact(
                cell,
                egui::Id::new((table.name.as_str(), j, i, "table-cell")),
                egui::Sense::hover(),
            )
            .on_hover_text(format!("Full parsed value: {value}"));
        }
    }
    let to_screen = |x: f64, y: f64| {
        Pos2::new(
            grid.x + (x as f32 + 0.5) * w,
            grid.y + (ny as f32 - 0.5 - y as f32) * h,
        )
    };
    for point in maps::trail(
        table,
        data,
        playback.position,
        playback.sample_index,
        seconds,
        1500,
    ) {
        painter.circle_filled(
            to_screen(point.x, point.y),
            (h * 0.15).clamp(1.0, 3.5),
            Color32::from_white_alpha((point.opacity * 190.0) as u8),
        );
    }
    let current = playback.sample_index.and_then(|i| {
        Some((
            data.columns[table.rpm_id].get(i).copied().flatten()?,
            data.columns[table.load_id].get(i).copied().flatten()?,
        ))
    });
    match current {
        None => {
            ui.label("Operating point unavailable: RPM or fuel-load sample is missing.");
        }
        Some((rpm, load)) => match table.position(rpm, load) {
            Some((x, y)) => {
                let p = to_screen(x, y);
                let radius = (h * 0.30).clamp(3.0, 7.0);
                painter.circle_filled(p, radius, Color32::WHITE);
                painter.circle_stroke(p, radius + 1.5, Stroke::new(2.0, Color32::BLACK));
            }
            None => {
                ui.label(format!(
                    "Operating point outside table range: {} rpm, {} kPa",
                    format_table_number(rpm),
                    format_table_number(load)
                ));
            }
        },
    }
}
fn contents(
    ui: &mut egui::Ui,
    result: Option<&Result<Table, String>>,
    data: Option<&Dataset>,
    playback: &Playback,
    heatmap: bool,
    seconds: f64,
    style: HeatmapStyle,
) {
    match (result, data) {
        (_, None) => {
            ui.label("Open a log to inspect a source table.");
        }
        (Some(Ok(table)), Some(data)) => {
            if table.ignored_duplicate_rpm > 0 {
                ui.label(format!(
                    "Ignored {} repeated RPM column(s); kept the first column at each breakpoint.",
                    table.ignored_duplicate_rpm
                ));
            }
            draw_table(ui, data, playback, table, heatmap, seconds, style)
        }
        (Some(Err(error)), Some(_)) => {
            ui.label(format!("Table unavailable: {error}"));
        }
        (None, Some(_)) => {
            ui.label("Source table has not been loaded.");
        }
    }
}
fn sanitize_preferences(prefs: &mut MapPreferences) {
    if !prefs.trail_seconds.is_finite() {
        prefs.trail_seconds = 5.0;
    }
    prefs.trail_seconds = prefs.trail_seconds.clamp(0.1, 60.0);
    prefs.ve_choice = prefs.ve_choice.min(1);
}

pub fn ve_panel(
    ui: &mut egui::Ui,
    data: Option<&Dataset>,
    playback: &Playback,
    state: &MapState,
    prefs: &mut MapPreferences,
) {
    sanitize_preferences(prefs);
    ui.heading("VE table");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut prefs.ve_choice, 0, "VE 1");
        ui.selectable_value(&mut prefs.ve_choice, 1, "VE 2");
    });
    contents(
        ui,
        state.ve.get(prefs.ve_choice).and_then(Option::as_ref),
        data,
        playback,
        prefs.heatmap,
        prefs.trail_seconds,
        HeatmapStyle {
            reference: 100.0,
            invert: false,
        },
    );
}

pub fn afr_panel(
    ui: &mut egui::Ui,
    data: Option<&Dataset>,
    playback: &Playback,
    state: &MapState,
    prefs: &mut MapPreferences,
) {
    sanitize_preferences(prefs);
    contents(
        ui,
        state.afr.as_ref(),
        data,
        playback,
        prefs.heatmap,
        prefs.trail_seconds,
        HeatmapStyle {
            reference: 14.7,
            invert: true,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{format_table_number, spectrum_color};

    #[test]
    fn table_values_use_three_significant_figures() {
        assert_eq!(format_table_number(1.0), "1");
        assert_eq!(format_table_number(14.76), "14.8");
        assert_eq!(format_table_number(0.012345), "0.0123");
        assert_eq!(format_table_number(0.000001), "1.00e-6");
    }

    #[test]
    fn reference_value_uses_the_blue_center_region() {
        let color = spectrum_color(100.0, 80.0, 120.0, 100.0, false);
        assert!(color.b() > color.g());
        assert!(color.g() > color.r());
    }
}
