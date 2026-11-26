use crate::{
    log::{Dataset, FieldKind},
    playback::Playback,
};
use eframe::egui::{self, Color32};
use egui_plot::{Legend, Line, Plot, PlotBounds, PlotPoint, Points, VLine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type DefaultPlotGroup<'a> = (&'a str, bool, &'a [(&'a str, Option<&'a str>)]);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    pub unit: String,
    pub occurrence: usize,
    pub normalized: bool,
}
impl Series {
    fn from_id(data: &Dataset, id: usize) -> Self {
        let f = &data.fields[id];
        let occurrence = data
            .field_ids(&f.name)
            .iter()
            .filter(|&&i| {
                i < id
                    && data.fields[i].unit == f.unit
                    && matches!(data.fields[i].kind, FieldKind::Numeric { .. })
            })
            .count();
        Self {
            name: f.name.clone(),
            unit: f.unit.clone(),
            occurrence,
            normalized: false,
        }
    }
    fn id(&self, data: &Dataset) -> Option<usize> {
        data.field_ids(&self.name)
            .iter()
            .copied()
            .filter(|&i| {
                data.fields[i].unit == self.unit
                    && matches!(data.fields[i].kind, FieldKind::Numeric { .. })
            })
            .nth(self.occurrence)
    }
    fn label(&self) -> String {
        if self.unit.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.unit)
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Group {
    name: String,
    fields: Vec<Series>,
    stacked: bool,
    #[serde(skip)]
    reset: bool,
}
struct Geometry {
    start: usize,
    end: usize,
    budget: usize,
    normalized: bool,
    segments: Vec<Vec<PlotPoint>>,
}
struct Scatter {
    ids: [usize; 3],
    low: f64,
    high: f64,
    bins: Vec<Vec<PlotPoint>>,
    omitted: usize,
    sampled: usize,
    range: [f64; 2],
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PlotState {
    groups: Vec<Group>,
    #[serde(skip)]
    cache: HashMap<(usize, usize), Geometry>,
    #[serde(skip)]
    scatter: Option<Scatter>,
    low: f64,
    high: f64,
    time_range: Option<[f64; 2]>,
    pub scatter_tab: bool,
    #[serde(skip)]
    reset_scatter: bool,
}
impl Default for PlotState {
    fn default() -> Self {
        Self {
            groups: vec![Group::default()],
            cache: HashMap::new(),
            scatter: None,
            low: 10.0,
            high: 18.0,
            time_range: None,
            scatter_tab: false,
            reset_scatter: false,
        }
    }
}

fn groups_from_defaults(data: &Dataset, defaults: &[DefaultPlotGroup<'_>]) -> Vec<Group> {
    defaults
        .iter()
        .filter_map(|(name, stacked, fields)| {
            let fields: Vec<_> = fields
                .iter()
                .filter_map(|(field_name, unit)| {
                    data.field_ids(field_name)
                        .iter()
                        .copied()
                        .find(|&id| {
                            matches!(data.fields[id].kind, FieldKind::Numeric { .. })
                                && unit.is_none_or(|unit| data.fields[id].unit == unit)
                        })
                        .map(|id| Series::from_id(data, id))
                })
                .collect();
            (!fields.is_empty()).then(|| Group {
                name: (*name).into(),
                fields,
                stacked: *stacked,
                reset: true,
            })
        })
        .collect()
}

fn groups_match(actual: &[Group], expected: &[Group]) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            actual.name == expected.name
                && actual.stacked == expected.stacked
                && actual.fields == expected.fields
        })
}

impl PlotState {
    pub fn sanitize(&mut self) {
        if self.groups.is_empty() {
            self.groups.push(Group::default());
        }
        if !self.low.is_finite() || !self.high.is_finite() || self.low >= self.high {
            self.low = 10.0;
            self.high = 18.0;
        }
        if self.time_range.is_some_and(|range| {
            !range[0].is_finite() || !range[1].is_finite() || range[0] >= range[1]
        }) {
            self.time_range = None;
        }
        self.clear_dataset_cache();
    }
    pub fn set_time_range(&mut self, range: Option<[f64; 2]>) {
        if self.time_range != range {
            self.time_range = range;
            self.cache.clear();
            self.scatter = None;
            self.reset_scatter = true;
            for group in &mut self.groups {
                group.reset = true;
            }
        }
    }
    pub fn time_range(&self) -> Option<[f64; 2]> {
        self.time_range
    }
    pub fn clear_dataset_cache(&mut self) {
        self.cache.clear();
        self.scatter = None;
        for g in &mut self.groups {
            g.reset = true;
        }
    }
    pub fn add(&mut self, data: &Dataset, id: usize) {
        if matches!(data.fields[id].kind, FieldKind::Numeric { .. }) {
            self.groups[0].fields.push(Series::from_id(data, id));
        }
    }

    /// Populate the first-run plot layout from source channels present in the log.
    /// Existing user groups, including intentionally empty groups, are preserved.
    pub fn initialize_defaults(&mut self, data: &Dataset) {
        let old_defaults: [DefaultPlotGroup<'_>; 5] = [
            (
                "RPM / AFR / VE",
                true,
                &[
                    ("RPM", Some("rpm")),
                    ("AFR", Some("O2")),
                    ("VE _Current", Some("%")),
                ],
            ),
            (
                "CLT / WUE / Fan",
                true,
                &[("CLT", None), ("Gwarm", Some("%")), ("Fan", None)],
            ),
            (
                "Ignition timing",
                false,
                &[("Advance _Current", Some("deg"))],
            ),
            ("MAP", false, &[("MAP", Some("kpa"))]),
            ("Throttle", false, &[("TPS", Some("%"))]),
        ];
        let is_empty = self.groups.iter().all(|group| group.fields.is_empty());
        let is_previous_default =
            groups_match(&self.groups, &groups_from_defaults(data, &old_defaults));
        if !is_empty && !is_previous_default {
            return;
        }
        let defaults: [DefaultPlotGroup<'_>; 3] = [
            (
                "AFR",
                false,
                &[("AFR", Some("O2")), ("AFR Target", Some("O2"))],
            ),
            (
                "Load",
                true,
                &[
                    ("RPM", Some("rpm")),
                    ("TPS", Some("%")),
                    ("MAP", Some("kpa")),
                    ("VE _Current", Some("%")),
                ],
            ),
            (
                "Fan",
                true,
                &[("CLT", None), ("Gwarm", Some("%")), ("Fan", None)],
            ),
        ];
        let groups = groups_from_defaults(data, &defaults);
        self.groups = if groups.is_empty() {
            vec![Group::default()]
        } else {
            groups
        };
        self.clear_dataset_cache();
    }

    #[cfg(test)]
    fn default_group_summary(&self) -> Vec<(String, bool, Vec<String>)> {
        self.groups
            .iter()
            .map(|group| {
                (
                    group.name.clone(),
                    group.stacked,
                    group
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect(),
                )
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn test_summary(&self) -> (usize, usize, bool, f64, f64) {
        (
            self.groups.len(),
            self.groups.first().map_or(0, |group| group.fields.len()),
            self.groups.first().is_some_and(|group| group.stacked),
            self.low,
            self.high,
        )
    }
}
fn palette(i: usize) -> Color32 {
    [
        Color32::from_rgb(81, 188, 248),
        Color32::from_rgb(255, 180, 79),
        Color32::from_rgb(128, 220, 130),
        Color32::from_rgb(235, 129, 198),
        Color32::from_rgb(216, 211, 107),
        Color32::from_rgb(174, 158, 255),
    ][i % 6]
}
fn series_color(series: &Series) -> Color32 {
    let mut hash = 2166136261u32;
    for byte in series
        .name
        .bytes()
        .chain(series.unit.bytes())
        .chain(series.occurrence.to_le_bytes())
    {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16777619);
    }
    palette(hash as usize)
}
fn indices_for_range(elapsed: &[f64], range: [f64; 2]) -> (usize, usize) {
    (
        elapsed.partition_point(|&time| time < range[0]),
        elapsed.partition_point(|&time| time <= range[1]),
    )
}
fn fit_y_bounds(
    data: &Dataset,
    ids: impl Iterator<Item = usize>,
    range: [f64; 2],
    dual_axis: bool,
) -> [f64; 2] {
    if dual_axis {
        return [0.0, 1.0];
    }
    let (start, end) = indices_for_range(&data.elapsed, range);
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for id in ids {
        for value in data.columns[id][start..end].iter().flatten().copied() {
            if value.is_finite() {
                min = min.min(value);
                max = max.max(value);
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return [-1.0, 1.0];
    }
    let padding = if max > min {
        (max - min) * 0.05
    } else {
        (min.abs() * 0.05).max(1.0)
    };
    [min - padding, max + padding]
}
/// Keeps first, minimum, maximum and last recorded points per bucket in source order.
/// Missing values always break a line; excess disconnected segments are omitted.
pub fn geometry(
    data: &Dataset,
    id: usize,
    start: usize,
    end: usize,
    budget: usize,
    normalized: bool,
) -> Vec<Vec<PlotPoint>> {
    let mut result = Vec::new();
    let mut segment = Vec::new();
    let mut run = Vec::new();
    let end = end.min(data.elapsed.len());
    let start = start.min(end);
    let budget = budget.max(8);
    let bucket = (end - start).div_ceil((budget / 4).max(1)).max(1);
    let stats = data.statistics(id);
    let transform = |value: f64| {
        if normalized {
            stats.map_or(value, |s| {
                if s.max > s.min {
                    (value - s.min) / (s.max - s.min)
                } else {
                    0.5
                }
            })
        } else {
            value
        }
    };
    let flush = |run: &mut Vec<(usize, f64)>, segment: &mut Vec<PlotPoint>| {
        if run.is_empty() {
            return;
        }
        let mut picks = [
            0,
            run.iter()
                .enumerate()
                .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
                .unwrap()
                .0,
            run.iter()
                .enumerate()
                .max_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
                .unwrap()
                .0,
            run.len() - 1,
        ];
        picks.sort_unstable();
        let mut last = None;
        for pick in picks {
            if last != Some(pick) {
                let (i, v) = run[pick];
                segment.push(PlotPoint::new(data.elapsed[i], transform(v)));
                last = Some(pick);
            }
        }
        run.clear();
    };
    let mut retained = 0usize;
    for i in start..end {
        match data.columns[id][i] {
            Some(v) => {
                if i > start && (i - start) % bucket == 0 {
                    flush(&mut run, &mut segment);
                }
                run.push((i, v));
            }
            None => {
                flush(&mut run, &mut segment);
                if !segment.is_empty() {
                    retained += segment.len();
                    result.push(std::mem::take(&mut segment));
                }
                if retained >= budget {
                    break;
                }
            }
        }
    }
    flush(&mut run, &mut segment);
    if !segment.is_empty() && retained < budget {
        result.push(segment);
    }
    // Keep visible-range extrema even when a highly fragmented series exceeds the segment budget.
    let visible = (start..end).filter_map(|i| data.columns[id][i].map(|v| (i, v)));
    let minimum = visible.clone().min_by(|a, b| a.1.total_cmp(&b.1));
    let maximum = visible.max_by(|a, b| a.1.total_cmp(&b.1));
    for (i, value) in [minimum, maximum].into_iter().flatten() {
        let point = PlotPoint::new(data.elapsed[i], transform(value));
        if !result
            .iter()
            .flatten()
            .any(|p| p.x == point.x && p.y == point.y)
        {
            result.push(vec![point]);
        }
    }
    result
}
fn unique(data: &Dataset, name: &str, unit: &str) -> Option<usize> {
    let ids: Vec<_> = data
        .field_ids(name)
        .iter()
        .copied()
        .filter(|&i| {
            data.fields[i].unit == unit && matches!(data.fields[i].kind, FieldKind::Numeric { .. })
        })
        .collect();
    (ids.len() == 1).then(|| ids[0])
}
fn make_scatter(data: &Dataset, ids: [usize; 3], low: f64, high: f64, range: [f64; 2]) -> Scatter {
    let mut bins = vec![Vec::new(); 16];
    let mut omitted = 0;
    let mut sampled = 0;
    let (start, end) = indices_for_range(&data.elapsed, range);
    let stride = end.saturating_sub(start).div_ceil(8000).max(1);
    for i in start..end {
        let (Some(x), Some(y), Some(afr)) = (
            data.columns[ids[0]][i],
            data.columns[ids[1]][i],
            data.columns[ids[2]][i],
        ) else {
            omitted += 1;
            continue;
        };
        if (i - start) % stride != 0 {
            continue;
        }
        let bin = (((afr - low) / (high - low)).clamp(0.0, 1.0) * 15.0) as usize;
        bins[bin].push(PlotPoint::new(x, y));
        sampled += 1;
    }
    Scatter {
        ids,
        low,
        high,
        bins,
        omitted,
        sampled,
        range,
    }
}
pub fn show_time_series(
    ui: &mut egui::Ui,
    data: &Dataset,
    playback: &Playback,
    state: &mut PlotState,
) -> Option<f64> {
    let mut seek = None;
    let range = state.time_range.unwrap_or([0.0, data.duration()]);
    ui.heading("Time series");
    ui.horizontal(|ui| {
        if crate::ui::icons::button(ui, crate::ui::icons::Icon::Add, "Add plot group", false)
            .clicked()
        {
            state.groups.push(Group::default());
        }
    });
    let mut remove_group = None;
    for (group_id, group) in state.groups.iter_mut().enumerate() {
        ui.group(|ui| {
            let default_name = format!("G{}", group_id + 1);
            let title = if group.name.is_empty() {
                default_name.clone()
            } else {
                group.name.clone()
            };
            ui.strong(title);
            egui::CollapsingHeader::new("Configure")
                .id_salt(("plot-config", group_id))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut group.name)
                                .hint_text(default_name)
                                .desired_width(120.0),
                        );
                        if crate::ui::icons::button(
                            ui,
                            crate::ui::icons::Icon::Stack,
                            "Toggle stacked plots",
                            group.stacked,
                        )
                        .clicked()
                        {
                            group.stacked = !group.stacked;
                            group.reset = true;
                        }
                        if crate::ui::icons::button(
                            ui,
                            crate::ui::icons::Icon::Fit,
                            "Fit to log / reset view",
                            false,
                        )
                        .clicked()
                        {
                            group.reset = true;
                            state.cache.retain(|(g, _), _| *g != group_id);
                        }
                        if group_id > 0
                            && crate::ui::icons::button(
                                ui,
                                crate::ui::icons::Icon::Remove,
                                "Remove plot group",
                                false,
                            )
                            .clicked()
                        {
                            remove_group = Some(group_id);
                        }
                    });
                    let mut remove = None;
                    for (i, series) in group.fields.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let label = ui.add(egui::Label::new(series.label()).truncate());
                            if let Some(id) = series.id(data) {
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
                            if crate::ui::icons::button(
                                ui,
                                crate::ui::icons::Icon::RightAxis,
                                "Toggle right Y axis",
                                series.normalized,
                            )
                            .clicked()
                            {
                                series.normalized = !series.normalized;
                                group.reset = true;
                            }
                            if crate::ui::icons::button(
                                ui,
                                crate::ui::icons::Icon::Remove,
                                "Remove field",
                                false,
                            )
                            .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        group.fields.remove(i);
                        state.cache.retain(|(g, _), _| *g != group_id);
                    }
                    egui::ComboBox::from_id_salt(("plot-add", group_id))
                        .selected_text("+ Field")
                        .show_ui(ui, |ui| {
                            for f in &data.fields {
                                if matches!(f.kind, FieldKind::Numeric { .. })
                                    && ui.button(format!("{} {}", f.name, f.unit)).clicked()
                                {
                                    group.fields.push(Series::from_id(data, f.id));
                                    ui.close_menu();
                                }
                            }
                        });
                });
            if group.fields.is_empty() {
                ui.label("Choose numeric fields to plot.");
                return;
            }
            let panels = if group.stacked { group.fields.len() } else { 1 };
            for panel in 0..panels {
                let selected: Vec<_> = group
                    .fields
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !group.stacked || *i == panel)
                    .collect();
                let left_stats = selected
                    .iter()
                    .find(|(_, s)| !s.normalized)
                    .or_else(|| selected.first())
                    .and_then(|(_, s)| s.id(data))
                    .and_then(|id| data.statistics(id));
                let right_stats = selected
                    .iter()
                    .find(|(_, s)| s.normalized)
                    .and_then(|(_, s)| s.id(data))
                    .and_then(|id| data.statistics(id));
                let dual_axis = right_stats.is_some();
                let fit_y = fit_y_bounds(
                    data,
                    selected.iter().filter_map(|(_, series)| series.id(data)),
                    range,
                    dual_axis,
                );
                let left_label = selected
                    .iter()
                    .find(|(_, series)| !series.normalized)
                    .or_else(|| selected.first())
                    .map(|(_, series)| series.label())
                    .unwrap_or_default();
                let right_label = selected
                    .iter()
                    .find(|(_, series)| series.normalized)
                    .map(|(_, series)| series.label());
                ui.horizontal(|ui| {
                    ui.label(format!("Left Y: {left_label}"));
                    if let Some(label) = &right_label {
                        ui.label(format!("Right Y: {label}"));
                    }
                });
                let axis_padding = if dual_axis {
                    (ui.available_width() * 0.25).min(52.0)
                } else {
                    0.0
                };
                let mut plot = Plot::new(("series", group_id, panel))
                    .allow_scroll(false)
                    .allow_drag([true, false])
                    .allow_zoom([true, false])
                    .height(if group.stacked { 180.0 } else { 260.0 })
                    .width((ui.available_width() - 2.0 * axis_padding).max(40.0))
                    .legend(Legend::default())
                    .x_axis_label("Elapsed time (s)")
                    .y_axis_label(if dual_axis {
                        "Left axis (see ticks)"
                    } else {
                        "Recorded values"
                    })
                    .show_axes([true, !dual_axis])
                    .link_axis(egui::Id::new("shared-time-axis"), [true, false])
                    .include_x(range[0])
                    .include_x(range[1]);
                if dual_axis {
                    plot = plot.include_y(0.0).include_y(1.0);
                }
                if group.reset {
                    plot = plot.reset();
                }
                let result = ui
                    .horizontal(|ui| {
                        if dual_axis {
                            ui.add_space(axis_padding);
                        }
                        let result = plot.show(ui, |plot_ui| {
                            if group.reset {
                                plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                    [range[0], fit_y[0]],
                                    [range[1], fit_y[1]],
                                ));
                                plot_ui.set_auto_bounds(false);
                            }
                            let bounds = plot_ui.plot_bounds();
                            let (range_start, range_end) = indices_for_range(&data.elapsed, range);
                            let (mut start, mut end) = if group.reset {
                                (range_start, range_end)
                            } else {
                                (
                                    data.elapsed.partition_point(|&t| t < bounds.min()[0]),
                                    data.elapsed.partition_point(|&t| t <= bounds.max()[0]),
                                )
                            };
                            start = start.max(range_start).min(range_end);
                            end = end.min(range_end).max(start);
                            if start < end {
                                start = start.saturating_sub(1).max(range_start);
                                end = (end + 1).min(range_end);
                            }
                            let budget =
                                (plot_ui.response().rect.width().max(100.0) as usize).min(2000) * 4;
                            for (i, series) in &selected {
                                if let Some(id) = series.id(data) {
                                    let key = (group_id, *i);
                                    let update = state.cache.get(&key).is_none_or(|c| {
                                        c.start != start
                                            || c.end != end
                                            || c.budget != budget
                                            || c.normalized != dual_axis
                                    });
                                    if update {
                                        state.cache.insert(
                                            key,
                                            Geometry {
                                                start,
                                                end,
                                                budget,
                                                normalized: dual_axis,
                                                segments: geometry(
                                                    data, id, start, end, budget, dual_axis,
                                                ),
                                            },
                                        );
                                    }
                                }
                            }
                            for (i, series) in &selected {
                                if let Some(cached) = state.cache.get(&(group_id, *i)) {
                                    for (j, segment) in cached.segments.iter().enumerate() {
                                        let name = if j == 0 {
                                            series.label()
                                        } else {
                                            String::new()
                                        };
                                        if segment.len() == 1 {
                                            plot_ui.points(
                                                Points::new(segment.as_slice())
                                                    .name(name)
                                                    .color(series_color(series))
                                                    .radius(2.0)
                                                    .allow_hover(false),
                                            );
                                        } else {
                                            plot_ui.line(
                                                Line::new(segment.as_slice())
                                                    .name(name)
                                                    .color(series_color(series))
                                                    .allow_hover(false),
                                            );
                                        }
                                    }
                                }
                            }
                            plot_ui.vline(VLine::new(playback.position).color(Color32::WHITE));
                            let pointer = plot_ui.pointer_coordinate().map(|p| p.x);
                            let frame = *plot_ui.transform().frame();
                            (
                                if plot_ui.response().clicked() {
                                    pointer
                                } else {
                                    None
                                },
                                if plot_ui.response().hovered() {
                                    pointer
                                } else {
                                    None
                                },
                                frame,
                            )
                        });
                        if dual_axis {
                            ui.add_space(axis_padding);
                        }
                        result
                    })
                    .inner;
                if let Some(time) = result.inner.0 {
                    seek = Some(time);
                }
                if let Some(index) = result.inner.1.and_then(|time| data.sample_at(time)) {
                    let mut tip = format!("Recorded sample at {:.3} s", data.elapsed[index]);
                    for (_, series) in &selected {
                        let value = series.id(data).and_then(|id| data.columns[id][index]);
                        tip.push_str(&format!(
                            "\n{}: {}",
                            series.label(),
                            value.map_or_else(|| "N/A".into(), |v| format!("{v:.3}"))
                        ));
                    }
                    result.response.on_hover_text(tip);
                }
                let frame = result.inner.2;
                let painter = ui.painter();
                if dual_axis {
                    for tick in 0..=4 {
                        let fraction = tick as f64 / 4.0;
                        let y = frame.bottom() - frame.height() * tick as f32 / 4.0;
                        if let Some(stats) = left_stats {
                            let value = stats.min + (stats.max - stats.min) * fraction;
                            painter.text(
                                egui::pos2(frame.left() - 4.0, y),
                                egui::Align2::RIGHT_CENTER,
                                format!("{value:.3}"),
                                egui::FontId::proportional(9.0),
                                Color32::LIGHT_GRAY,
                            );
                        }
                        if let Some(stats) = right_stats {
                            let value = stats.min + (stats.max - stats.min) * fraction;
                            painter.text(
                                egui::pos2(frame.right() + 4.0, y),
                                egui::Align2::LEFT_CENTER,
                                format!("{value:.3}"),
                                egui::FontId::proportional(9.0),
                                Color32::LIGHT_GRAY,
                            );
                        }
                    }
                }
            }
            group.reset = false;
            for series in &group.fields {
                if let Some(id) = series.id(data) {
                    let value = playback.sample_index.and_then(|i| data.columns[id][i]);
                    ui.label(format!(
                        "{}: {}{}",
                        series.label(),
                        value.map_or_else(|| "N/A".into(), |v| format!("{v:.3}")),
                        if series.normalized {
                            " (right axis)"
                        } else {
                            ""
                        }
                    ));
                } else {
                    ui.label(format!("{}: field unavailable", series.label()));
                }
            }
        });
    }
    if let Some(i) = remove_group {
        state.groups.remove(i);
        state.cache.clear();
    }
    seek
}

pub fn show_scatter(ui: &mut egui::Ui, data: &Dataset, playback: &Playback, state: &mut PlotState) {
    ui.heading("RPM / MAP colored by AFR");
    let verified = crate::analysis::derived::verified_firmware_semantics(data);
    let ids = (
        unique(data, "RPM", "rpm"),
        unique(data, "MAP", "kpa"),
        unique(data, "AFR", "O2"),
    );
    let (Some(rpm), Some(map), Some(afr)) = ids else {
        ui.label("Unavailable: unique RPM (rpm), MAP (kpa), and AFR (O2) channels are required.");
        return;
    };
    if !verified {
        ui.label("Unavailable: measured AFR channel semantics are not verified for this firmware.");
        return;
    }
    ui.horizontal(|ui| {
        ui.label("AFR color range");
        ui.add(egui::DragValue::new(&mut state.low).speed(0.1));
        ui.add(egui::DragValue::new(&mut state.high).speed(0.1));
        if ui.button("Reset scatter view").clicked() {
            state.reset_scatter = true;
        }
    });
    if !state.low.is_finite() || !state.high.is_finite() || state.low >= state.high {
        ui.label("Color maximum must exceed minimum.");
        return;
    }
    let ids = [rpm, map, afr];
    let range = state.time_range.unwrap_or([0.0, data.duration()]);
    if state.scatter.as_ref().is_none_or(|s| {
        s.ids != ids || s.low != state.low || s.high != state.high || s.range != range
    }) {
        state.scatter = Some(make_scatter(data, ids, state.low, state.high, range));
    }
    let scatter = state.scatter.as_ref().unwrap();
    ui.label(format!(
        "AFR (O2): reversed heatmap; 14.7 blue; range {:.1} to {:.1} · {} omitted for missing values · {} points drawn",
        state.low, state.high, scatter.omitted, scatter.sampled
    ));
    ui.horizontal(|ui| {
        ui.label(format!("{:.1}", state.low));
        for i in 0..16 {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 10.0), egui::Sense::hover());
            let value = state.low + (state.high - state.low) * i as f64 / 15.0;
            ui.painter().rect_filled(
                rect,
                0.0,
                crate::ui::maps::afr_color(value, state.low, state.high),
            );
        }
        ui.label(format!("{:.1} O2", state.high));
    });
    let mut plot = Plot::new("afr-scatter")
        .allow_scroll(false)
        .allow_drag([true, false])
        .allow_zoom([true, false])
        .height(280.0)
        .x_axis_label("RPM (rpm)")
        .y_axis_label("MAP (kpa)");
    if state.reset_scatter {
        plot = plot.reset();
        state.reset_scatter = false;
    }
    plot.show(ui, |p| {
        for (i, bin) in scatter.bins.iter().enumerate() {
            let value = state.low + (state.high - state.low) * i as f64 / 15.0;
            p.points(
                Points::new(bin.as_slice())
                    .color(crate::ui::maps::afr_color(value, state.low, state.high))
                    .radius(2.0)
                    .allow_hover(false),
            );
        }
        if let Some(i) = playback
            .sample_index
            .filter(|_| playback.position >= range[0] && playback.position <= range[1])
        {
            if let (Some(x), Some(y), Some(_)) = (
                data.columns[rpm][i],
                data.columns[map][i],
                data.columns[afr][i],
            ) {
                p.points(Points::new(vec![[x, y]]).radius(6.0).color(Color32::WHITE));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_time_range_includes_recorded_rows_at_both_boundaries() {
        let elapsed = [0.0, 1.0, 1.0, 2.5, 4.0];
        let (start, end) = indices_for_range(&elapsed, [1.0, 2.5]);
        assert_eq!(&elapsed[start..end], &[1.0, 1.0, 2.5]);
    }

    #[test]
    fn defaults_use_the_requested_plot_groups_and_stack_modes() {
        let data = crate::log::parse(include_bytes!("../../samples/demo.mlg")).unwrap();
        let mut state = PlotState::default();
        state.initialize_defaults(&data);
        assert_eq!(
            state.default_group_summary(),
            vec![
                (
                    "AFR".to_owned(),
                    false,
                    vec!["AFR".to_owned(), "AFR Target".to_owned()]
                ),
                (
                    "Load".to_owned(),
                    true,
                    vec![
                        "RPM".to_owned(),
                        "TPS".to_owned(),
                        "MAP".to_owned(),
                        "VE _Current".to_owned()
                    ]
                ),
                (
                    "Fan".to_owned(),
                    true,
                    vec!["CLT".to_owned(), "Gwarm".to_owned(), "Fan".to_owned()]
                )
            ]
        );
    }

    #[derive(Default)]
    struct MemoryStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn plot_preferences_round_trip_and_invalid_values_are_sanitized() {
        let data = crate::log::parse(include_bytes!("../../tests/fixtures/sanitized.mlg")).unwrap();
        let mut state = PlotState::default();
        state.add(&data, data.field_ids("RPM")[0]);
        state.add(&data, data.field_ids("MAP")[0]);
        state.groups[0].stacked = true;
        state.low = 9.5;
        state.high = 19.5;
        state.set_time_range(Some([12.5, 42.0]));

        let mut storage = MemoryStorage::default();
        eframe::set_value(&mut storage, "plot-settings-v1", &state);
        let mut loaded: PlotState = eframe::get_value(&storage, "plot-settings-v1").unwrap();
        loaded.sanitize();
        assert_eq!(loaded.test_summary(), (1, 2, true, 9.5, 19.5));
        assert_eq!(loaded.time_range, Some([12.5, 42.0]));
        assert!(loaded.groups[0].fields[0].id(&data).is_some());

        loaded.low = f64::NAN;
        loaded.high = f64::NEG_INFINITY;
        loaded.sanitize();
        assert_eq!(loaded.test_summary().3, 10.0);
        assert_eq!(loaded.test_summary().4, 18.0);
    }
}
