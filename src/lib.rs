pub mod analysis;
pub mod log;
pub mod playback;
pub mod ui;

use eframe::egui;
use log::Dataset;
use std::sync::mpsc::{self, Receiver};

type LoadResult = Result<Option<LoadedLog>, String>;
type ExportResult = Result<bool, String>;

struct LoadedLog {
    name: String,
    bytes: Vec<u8>,
    dataset: Dataset,
    load_ms: Option<f64>,
    sample_name: Option<String>,
    auto_play: bool,
}

#[derive(Clone, Copy, Default)]
struct LinkView {
    playhead: Option<f64>,
    start: Option<f64>,
    end: Option<f64>,
}

#[cfg(all(target_arch = "wasm32", browser_smoke))]
thread_local! {
    static SMOKE_IMPORT: std::cell::RefCell<Option<(String, Vec<u8>, f64)>> = const { std::cell::RefCell::new(None) };
    static SMOKE_CONTEXT: std::cell::RefCell<Option<egui::Context>> = const { std::cell::RefCell::new(None) };
}

#[derive(Default)]
pub struct LogViewer {
    dataset: Option<Dataset>,
    source_bytes: Vec<u8>,
    filename: String,
    error: Option<String>,
    notice: Option<String>,
    loading: Option<Receiver<LoadResult>>,
    exporting: Option<Receiver<ExportResult>>,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    sample_name: Option<String>,
    link_view: Option<LinkView>,
    search: String,
    playback: playback::Playback,
    settings: GaugeSettings,
    plots: ui::plots::PlotState,
    maps: ui::maps::MapState,
    theme_applied: bool,
    #[cfg(all(target_arch = "wasm32", browser_smoke))]
    smoke_load_ms: Option<f64>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct GaugeSettings {
    version: u32,
    initialized: bool,
    gauges: Vec<ui::gauges::Gauge>,
    maps: ui::maps::MapPreferences,
    speed: f64,
    dark_theme: bool,
    plots_tab: bool,
}
impl Default for GaugeSettings {
    fn default() -> Self {
        Self {
            version: 1,
            initialized: false,
            gauges: Vec::new(),
            maps: ui::maps::MapPreferences::default(),
            speed: 1.0,
            dark_theme: true,
            plots_tab: false,
        }
    }
}
fn initial_gauges(data: &Dataset) -> Vec<ui::gauges::Gauge> {
    [
        ("RPM", "rpm"),
        ("TPS", "%"),
        ("PW", "ms"),
        ("Duty Cycle", ""),
        ("Advance _Current", "deg"),
        ("IAT", ""),
        ("CLT", ""),
        ("MAP", "kpa"),
        ("AFR", "O2"),
        ("AFR Target", "O2"),
        ("AFR error (%)", "%"),
        ("Battery V", "V"),
        ("Gammae", "%"),
    ]
    .iter()
    .filter_map(|(name, unit)| {
        data.field_ids(name)
            .iter()
            .copied()
            .find(|&id| data.fields[id].unit == *unit)
    })
    .filter_map(|id| ui::gauges::Gauge::from_field(data, id))
    .collect()
}

impl LogViewer {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(all(target_arch = "wasm32", browser_smoke))]
        SMOKE_CONTEXT.with(|context| *context.borrow_mut() = Some(cc.egui_ctx.clone()));
        let settings: GaugeSettings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "gauge-settings"))
            .filter(|s: &GaugeSettings| s.version == 1)
            .unwrap_or_default();
        let mut plots: ui::plots::PlotState = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "plot-settings-v1"))
            .unwrap_or_default();
        plots.sanitize();
        let mut playback = playback::Playback::default();
        playback.set_speed(settings.speed);
        let app = Self {
            settings,
            plots,
            playback,
            ..Self::default()
        };
        #[cfg(target_arch = "wasm32")]
        {
            let mut app = app;
            app.load_sample_link(cc.egui_ctx.clone());
            app
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut app = app;
            app.load_default_demo();
            app
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn load_default_demo(&mut self) {
        let bytes = include_bytes!("../samples/demo.mlg").to_vec();
        if let Ok(data) = log::parse(&bytes) {
            self.install_loaded(LoadedLog {
                name: "demo.mlg".into(),
                bytes,
                dataset: data,
                load_ms: None,
                sample_name: Some("demo.mlg".into()),
                auto_play: true,
            });
        }
    }
    fn install_loaded(&mut self, loaded: LoadedLog) {
        let LoadedLog {
            name,
            bytes,
            dataset: data,
            load_ms,
            sample_name,
            auto_play,
        } = loaded;
        self.filename = name;
        self.source_bytes = bytes;
        self.sample_name = sample_name;
        self.playback.reset(&data);
        if let Some(view) = self.link_view.take() {
            let duration = data.duration();
            if view.start.is_some() || view.end.is_some() {
                let start = view.start.unwrap_or(0.0).min(duration);
                let end = view.end.unwrap_or(duration).min(duration);
                if start < end {
                    self.plots.set_time_range(Some([start, end]));
                }
            }
            if let Some(position) = view.playhead {
                self.playback.seek(position.min(duration), &data);
            }
        }
        self.plots.clear_dataset_cache();
        self.plots.initialize_defaults(&data);
        self.maps.load(&data);
        if !self.settings.initialized {
            self.settings.gauges = initial_gauges(&data);
            self.settings.initialized = true;
        }
        if auto_play {
            self.playback.toggle(&data);
        }
        self.dataset = Some(data);
        #[cfg(all(target_arch = "wasm32", browser_smoke))]
        {
            self.smoke_load_ms = load_ms;
        }
        #[cfg(not(all(target_arch = "wasm32", browser_smoke)))]
        let _ = load_ms;
        self.error = None;
        self.notice = None;
    }
    fn open(&mut self, ctx: &egui::Context) {
        self.link_view = None;
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        let ctx = ctx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let result = match rfd::FileDialog::new()
                .add_filter("ECU log", &["mlg"])
                .pick_file()
            {
                Some(path) => {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    std::fs::read(&path)
                        .map_err(|e| format!("Could not read {name}: {e}"))
                        .and_then(|bytes| {
                            log::parse(&bytes).map(|dataset| {
                                Some(LoadedLog {
                                    name,
                                    bytes,
                                    dataset,
                                    load_ms: None,
                                    sample_name: None,
                                    auto_play: false,
                                })
                            })
                        })
                }
                None => Ok(None),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = match rfd::AsyncFileDialog::new()
                .add_filter("ECU log", &["mlg"])
                .pick_file()
                .await
            {
                Some(file) => {
                    let name = file.file_name();
                    #[cfg(browser_smoke)]
                    let started = web_sys::window()
                        .and_then(|window| window.performance())
                        .map(|performance| performance.now());
                    let bytes = file.read().await;
                    log::parse(&bytes).map(|dataset| {
                        let elapsed = {
                            #[cfg(browser_smoke)]
                            {
                                started.and_then(|started| {
                                    web_sys::window()
                                        .and_then(|window| window.performance())
                                        .map(|performance| performance.now() - started)
                                })
                            }
                            #[cfg(not(browser_smoke))]
                            {
                                None
                            }
                        };
                        Some(LoadedLog {
                            name,
                            bytes,
                            dataset,
                            load_ms: elapsed,
                            sample_name: None,
                            auto_play: false,
                        })
                    })
                }
                None => Ok(None),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn export_crop(&mut self, ctx: &egui::Context) {
        let Some(data) = &self.dataset else { return };
        let range = self.plots.time_range().unwrap_or([0.0, data.duration()]);
        let bytes = match log::crop(&self.source_bytes, data, range[0], range[1]) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        let stem = std::path::Path::new(&self.filename)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("log");
        let suggested_name = format!("{stem}-crop.mlg");
        let (tx, rx) = mpsc::channel();
        self.exporting = Some(rx);
        let ctx = ctx.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .add_filter("ECU log", &["mlg"])
                .set_file_name(&suggested_name)
                .save_file()
                .map(|path| {
                    std::fs::write(path, bytes)
                        .map(|()| true)
                        .map_err(|error| format!("Could not save cropped log: {error}"))
                })
                .unwrap_or(Ok(false));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = match rfd::AsyncFileDialog::new()
                .add_filter("ECU log", &["mlg"])
                .set_file_name(&suggested_name)
                .save_file()
                .await
            {
                Some(file) => file
                    .write(&bytes)
                    .await
                    .map(|()| true)
                    .map_err(|error| format!("Could not save cropped log: {error}")),
                None => Ok(false),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn load_sample_link(&mut self, ctx: egui::Context) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let search = match window.location().search() {
            Ok(search) => search,
            Err(_) => return,
        };
        let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
            self.error = Some("Could not read sample-link parameters".into());
            return;
        };
        let name = params.get("sample").unwrap_or_else(|| "demo.mlg".into());
        if !valid_sample_name(&name) {
            self.error = Some("Sample name must be a .mlg filename without a path".into());
            return;
        }
        let parse_time = |key: &str| -> Result<Option<f64>, String> {
            params
                .get(key)
                .map(|value| {
                    value
                        .parse::<f64>()
                        .ok()
                        .filter(|time| time.is_finite() && *time >= 0.0)
                        .ok_or_else(|| format!("Invalid {key} time in sample link"))
                })
                .transpose()
        };
        let view = match (
            parse_time("playhead"),
            parse_time("start"),
            parse_time("end"),
        ) {
            (Ok(playhead), Ok(start), Ok(end)) => LinkView {
                playhead,
                start,
                end,
            },
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                self.error = Some(error);
                return;
            }
        };
        if view
            .start
            .zip(view.end)
            .is_some_and(|(start, end)| start >= end)
        {
            self.error = Some("Sample-link range start must be before end".into());
            return;
        }
        self.sample_name = Some(name.clone());
        self.link_view = Some(view);
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        wasm_bindgen_futures::spawn_local(async move {
            use web_sys::wasm_bindgen::JsCast;
            let result = async {
                let path = format!("./samples/{name}");
                let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&path))
                    .await
                    .map_err(|error| format!("Could not fetch sample: {error:?}"))?
                    .dyn_into::<web_sys::Response>()
                    .map_err(|_| "Sample request returned an invalid response".to_owned())?;
                if !response.ok() {
                    return Err(format!("Sample {name} returned HTTP {}", response.status()));
                }
                let buffer = wasm_bindgen_futures::JsFuture::from(
                    response
                        .array_buffer()
                        .map_err(|error| format!("Could not read sample response: {error:?}"))?,
                )
                .await
                .map_err(|error| format!("Could not read sample bytes: {error:?}"))?;
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                let dataset = log::parse(&bytes)?;
                Ok(LoadedLog {
                    name: name.clone(),
                    bytes,
                    dataset,
                    load_ms: None,
                    sample_name: Some(name),
                    auto_play: true,
                })
            }
            .await
            .map(Some);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn copy_sample_link(&self, ctx: &egui::Context) {
        let (Some(name), Some(data)) = (&self.sample_name, &self.dataset) else {
            return;
        };
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(href) = window.location().href() else {
            return;
        };
        let base = href.split(['?', '#']).next().unwrap_or(&href);
        let range = self.plots.time_range().unwrap_or([0.0, data.duration()]);
        ctx.copy_text(format!(
            "{base}?sample={name}&playhead={:.3}&start={:.3}&end={:.3}",
            self.playback.position, range[0], range[1]
        ));
    }
}

#[cfg(target_arch = "wasm32")]
fn valid_sample_name(name: &str) -> bool {
    name.ends_with(".mlg")
        && name[..name.len().saturating_sub(4)]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        && name.len() > 4
}
impl eframe::App for LogViewer {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.speed = self.playback.speed;
        eframe::set_value(storage, "gauge-settings", &self.settings);
        eframe::set_value(storage, "plot-settings-v1", &self.plots);
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(all(target_arch = "wasm32", browser_smoke))]
        if let Some((name, bytes, started)) = SMOKE_IMPORT.with(|file| file.borrow_mut().take()) {
            let load_ms = log::parse(&bytes).map(|dataset| {
                let ended = web_sys::window()
                    .and_then(|window| window.performance())
                    .map_or(started, |performance| performance.now());
                LoadedLog {
                    name,
                    bytes,
                    dataset,
                    load_ms: Some(ended - started),
                    sample_name: None,
                    auto_play: false,
                }
            });
            let (sender, receiver) = mpsc::channel();
            let result = load_ms.map(Some);
            let _ = sender.send(result);
            self.loading = Some(receiver);
        }
        if !self.theme_applied {
            ctx.set_visuals(if self.settings.dark_theme {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            self.theme_applied = true;
        }
        if let Some(rx) = &self.loading {
            match rx.try_recv() {
                Ok(result) => {
                    self.loading = None;
                    match result {
                        Ok(Some(loaded)) => {
                            self.install_loaded(loaded);
                        }
                        Ok(None) => {}
                        Err(e) => self.error = Some(e),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading = None;
                    self.error = Some("File selection failed unexpectedly".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.exporting {
            match rx.try_recv() {
                Ok(result) => {
                    self.exporting = None;
                    match result {
                        Ok(true) => {
                            self.error = None;
                            self.notice = Some("Cropped log saved".into());
                        }
                        Ok(false) => {}
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.exporting = None;
                    self.error = Some("Crop export did not complete".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(data) = &self.dataset {
            let now = ctx.input(|i| i.time);
            self.playback.tick(now, data);
            if self.playback.playing {
                ctx.request_repaint();
            }
            if !ctx.wants_keyboard_input() && ctx.memory(|m| m.focused().is_none()) {
                ctx.input(|i| {
                    if i.key_pressed(egui::Key::Space) {
                        self.playback.toggle(data);
                    }
                    if i.key_pressed(egui::Key::ArrowLeft) {
                        self.playback.seek(self.playback.position - 10.0, data);
                    }
                    if i.key_pressed(egui::Key::ArrowRight) {
                        self.playback.seek(self.playback.position + 10.0, data);
                    }
                    if i.key_pressed(egui::Key::Home) {
                        self.playback.seek(0.0, data);
                    }
                    if i.key_pressed(egui::Key::End) {
                        self.playback.seek(data.duration(), data);
                    }
                });
            }
        }
        egui::TopBottomPanel::top("app-header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let logo_menu = egui::menu::menu_custom_button(
                    ui,
                    egui::Button::new("")
                        .min_size(egui::vec2(34.0, 34.0))
                        .corner_radius(17.0),
                    |ui| {
                        if ui
                            .add_enabled(self.loading.is_none(), egui::Button::new("Open log…"))
                            .clicked()
                        {
                            ui.close_menu();
                            self.open(ctx);
                        }
                        if let Some(data) = &self.dataset {
                            let range = self.plots.time_range().unwrap_or([0.0, data.duration()]);
                            let rows = data.time_range(range[0], range[1]).len();
                            if ui
                                .add_enabled(
                                    self.exporting.is_none(),
                                    egui::Button::new(format!("Export crop ({rows} samples)…")),
                                )
                                .on_hover_text(
                                    "Save only recorded samples inside the blue timeline range",
                                )
                                .clicked()
                            {
                                ui.close_menu();
                                self.export_crop(ctx);
                            }
                        }
                        #[cfg(target_arch = "wasm32")]
                        if self.sample_name.is_some()
                            && ui
                                .button("Copy sample link at current time and range")
                                .clicked()
                        {
                            ui.close_menu();
                            self.copy_sample_link(ctx);
                        }
                    },
                );
                ui::icons::paint_logo(ui, logo_menu.response.rect);
                ui.label("Log Viewer");
                ui.separator();
                if !self.filename.is_empty() {
                    ui.add(egui::Label::new(&self.filename).truncate())
                        .on_hover_text(&self.filename);
                } else if self.loading.is_some() {
                    ui.label("Loading log…");
                } else {
                    ui.label("No log loaded");
                }
                if let Some(notice) = &self.notice {
                    ui.label(notice);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (icon, tip) = if self.settings.dark_theme {
                        (ui::icons::Icon::Sun, "Switch to light theme")
                    } else {
                        (ui::icons::Icon::Moon, "Switch to dark theme")
                    };
                    if ui::icons::button(ui, icon, tip, false).clicked() {
                        self.settings.dark_theme = !self.settings.dark_theme;
                        self.theme_applied = false;
                        ctx.request_repaint();
                    }
                });
            });
        });
        egui::TopBottomPanel::bottom("transport").show(ctx, |ui| {
            if let Some(data) = &self.dataset {
                ui.horizontal(|ui| {
                    ui.label(format!("{} samples", data.timestamps.len()));
                    ui.separator();
                    ui.label(format!("{:.3} s", data.duration()));
                    if let Some(diagnostic) = &data.metadata.tune_diagnostic {
                        ui.separator();
                        ui.add(egui::Label::new(format!("Tune: {diagnostic}")).truncate())
                            .on_hover_text(diagnostic);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let (play_icon, play_tip) = if self.playback.playing {
                        (ui::icons::Icon::Pause, "Pause playback")
                    } else {
                        (ui::icons::Icon::Play, "Start playback")
                    };
                    if ui::icons::button(ui, play_icon, play_tip, false).clicked() {
                        self.playback.toggle(data);
                    }
                    if ui.button("-10 s").clicked() {
                        self.playback.seek(self.playback.position - 10.0, data);
                    }
                    if ui.button("+10 s").clicked() {
                        self.playback.seek(self.playback.position + 10.0, data);
                    }
                    egui::ComboBox::from_id_salt("speed")
                        .selected_text(format!("{}x", self.playback.speed))
                        .show_ui(ui, |ui| {
                            for speed in playback::SPEEDS {
                                if ui
                                    .selectable_label(
                                        self.playback.speed == speed,
                                        format!("{speed}x"),
                                    )
                                    .clicked()
                                {
                                    self.playback.set_speed(speed);
                                }
                            }
                        });
                });
                let mut range = self.plots.time_range();
                if ui::timeline::show(ui, data, &mut self.playback, &mut range) {
                    self.plots.set_time_range(range);
                }
            }
        });
        let mut plot_seek = None;
        if self.dataset.is_some() {
            egui::SidePanel::left("field-values")
                .default_width(330.0)
                .min_width(250.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.heading("Fields");
                    ui.horizontal(|ui| {
                        if ui.button("Reset all settings").clicked() {
                            self.settings = GaugeSettings::default();
                            self.plots = ui::plots::PlotState::default();
                            if let Some(data) = &self.dataset {
                                self.settings.gauges = initial_gauges(data);
                                self.settings.initialized = true;
                                self.plots.initialize_defaults(data);
                            }
                            self.playback.set_speed(1.0);
                            self.theme_applied = false;
                            ctx.memory_mut(|memory| *memory = egui::Memory::default());
                            ctx.request_repaint();
                        }
                    });
                    if let Some(error) = &self.error {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("Could not load log: {error}"),
                        );
                    }
                    if let Some(data) = &self.dataset {
                        ui.horizontal(|ui| {
                            ui.label("Find field:");
                            ui.text_edit_singleline(&mut self.search);
                        });
                        egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new("fields")
                                    .striped(true)
                                    .min_col_width(12.0)
                                    .show(ui, |ui| {
                                        ui.strong("+");
                                        ui.strong("Field");
                                        ui.strong("Unit");
                                        ui.strong("Value");
                                        ui.end_row();
                                        let search = self.search.to_lowercase();
                                        for field in &data.fields {
                                            if !field.name.to_lowercase().contains(&search) {
                                                continue;
                                            }
                                            if ui
                                                .add_enabled(
                                                    matches!(
                                                        field.kind,
                                                        log::FieldKind::Numeric { .. }
                                                    ),
                                                    egui::Button::new("+").small(),
                                                )
                                                .on_hover_text("Add this field to the Gauges tab")
                                                .clicked()
                                            {
                                                if let Some(gauge) =
                                                    ui::gauges::Gauge::from_field(data, field.id)
                                                {
                                                    self.settings.gauges.push(gauge);
                                                }
                                            }
                                            ui.label(if field.name.is_empty() {
                                                format!("(unnamed {})", field.id)
                                            } else {
                                                field.name.clone()
                                            });
                                            ui.label(&field.unit);
                                            match self
                                                .playback
                                                .sample_index
                                                .and_then(|index| data.columns[field.id][index])
                                            {
                                                Some(value) => {
                                                    ui.label(ui::maps::format_table_number(value))
                                                        .on_hover_text(format!(
                                                            "Full parsed value: {value}"
                                                        ));
                                                }
                                                None => {
                                                    ui.label("N/A");
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                    }
                });
            egui::SidePanel::right("map-panels")
                .default_width(560.0)
                .min_width(350.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.settings.maps.heatmap, "Table heatmap");
                        ui.add(
                            egui::Slider::new(&mut self.settings.maps.trail_seconds, 0.1..=60.0)
                                .text("Trail seconds"),
                        );
                    });
                    let panel_height = (ui.available_height() - 8.0) * 0.5;
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(ui.available_width(), panel_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui::maps::ve_panel(
                                ui,
                                self.dataset.as_ref(),
                                &self.playback,
                                &self.maps,
                                &mut self.settings.maps,
                            );
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::Vec2::new(ui.available_width(), panel_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut self.plots.scatter_tab,
                                    false,
                                    "AFR table",
                                );
                                ui.selectable_value(
                                    &mut self.plots.scatter_tab,
                                    true,
                                    "Scatter plot",
                                );
                            });
                            if self.plots.scatter_tab {
                                if let Some(data) = &self.dataset {
                                    ui::plots::show_scatter(
                                        ui,
                                        data,
                                        &self.playback,
                                        &mut self.plots,
                                    );
                                } else {
                                    ui.label("Open a log to inspect the RPM/MAP scatter plot.");
                                }
                            } else {
                                ui::maps::afr_panel(
                                    ui,
                                    self.dataset.as_ref(),
                                    &self.playback,
                                    &self.maps,
                                    &mut self.settings.maps,
                                );
                            }
                        },
                    );
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.dataset.is_none() {
                let rect = egui::Rect::from_center_size(
                    ui.available_rect_before_wrap().center(),
                    egui::Vec2::new(164.0, 40.0),
                );
                if ui.put(rect, egui::Button::new("Open log…")).clicked() {
                    self.open(ctx);
                }
                return;
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.settings.plots_tab, false, "Gauges");
                ui.selectable_value(&mut self.settings.plots_tab, true, "Plots");
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(data) = &self.dataset {
                        if self.settings.plots_tab {
                            plot_seek = ui::plots::show_time_series(
                                ui,
                                data,
                                &self.playback,
                                &mut self.plots,
                            );
                            if !data.metadata.derived_diagnostics.is_empty() {
                                egui::CollapsingHeader::new("Unavailable derived fields").show(
                                    ui,
                                    |ui| {
                                        for diagnostic in &data.metadata.derived_diagnostics {
                                            ui.label(diagnostic);
                                        }
                                    },
                                );
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.heading("Gauges");
                                if ui.button("Reset gauge layout").clicked() {
                                    self.settings.gauges = initial_gauges(data);
                                }
                            });
                            if self.settings.gauges.is_empty() {
                                ui.label(
                                    "Add numeric fields from the left panel to create gauges.",
                                );
                            }
                            let change = ui::gauges::show_wrapped(
                                ui,
                                &mut self.settings.gauges,
                                data,
                                self.playback.sample_index,
                            );
                            if let Some((i, action)) = change {
                                match action {
                                    ui::gauges::Action::Remove => {
                                        self.settings.gauges.remove(i);
                                    }
                                    ui::gauges::Action::Up if i > 0 => {
                                        self.settings.gauges.swap(i, i - 1)
                                    }
                                    ui::gauges::Action::Down
                                        if i + 1 < self.settings.gauges.len() =>
                                    {
                                        self.settings.gauges.swap(i, i + 1)
                                    }
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        ui.label(
                            "Open a supported .mlg binary log to inspect its recorded fields.",
                        );
                    }
                });
        });
        if let (Some(time), Some(data)) = (plot_seek, &self.dataset) {
            self.playback.seek(time, data);
            ctx.request_repaint();
        }
        #[cfg(all(target_arch = "wasm32", browser_smoke))]
        publish_smoke_state(self);
    }
}

#[cfg(all(target_arch = "wasm32", browser_smoke))]
fn publish_smoke_state(app: &LogViewer) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(element) = document.get_element_by_id("log-viewer-smoke") else {
        return;
    };
    let data = app.dataset.as_ref();
    let index = app
        .playback
        .sample_index
        .map_or_else(|| "none".into(), |i| i.to_string());
    let current = data
        .and_then(|data| app.playback.sample_index.map(|i| (data, i)))
        .and_then(|(data, i)| {
            data.field_ids("RPM")
                .first()
                .and_then(|&id| data.columns[id][i])
        })
        .map_or_else(|| "none".into(), |value| value.to_string());
    let (afr_diagnostic, ve_diagnostic) = app.maps.smoke_summary();
    for (key, value) in [
        ("loaded", data.is_some().to_string()),
        ("filename", app.filename.clone()),
        ("sample-index", index),
        (
            "sample-count",
            data.map_or(0, |d| d.timestamps.len()).to_string(),
        ),
        ("rpm", current),
        ("afr-visible", data.is_some().to_string()),
        ("ve-visible", data.is_some().to_string()),
        ("afr-diagnostic", afr_diagnostic),
        ("ve-diagnostic", ve_diagnostic),
        ("error", app.error.clone().unwrap_or_default()),
        ("loading", app.loading.is_some().to_string()),
        (
            "load-ms",
            app.smoke_load_ms
                .map_or_else(String::new, |ms| format!("{ms:.3}")),
        ),
    ] {
        let _ = element.set_attribute(key, &value);
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn start_web() -> Result<(), eframe::wasm_bindgen::JsValue> {
    let window = web_sys::window().unwrap();
    #[cfg(browser_smoke)]
    {
        use eframe::wasm_bindgen::{closure::Closure, JsValue};
        let import = Closure::wrap(Box::new(
            move |name: JsValue, buffer: JsValue, started: JsValue| {
                let name = name.as_string().unwrap_or_else(|| "smoke.mlg".to_owned());
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                let started = started.as_f64().unwrap_or_default();
                SMOKE_IMPORT.with(|file| *file.borrow_mut() = Some((name, bytes, started)));
                SMOKE_CONTEXT.with(|context| {
                    if let Some(context) = context.borrow().as_ref() {
                        context.request_repaint();
                    }
                });
            },
        ) as Box<dyn FnMut(JsValue, JsValue, JsValue)>);
        js_sys::Reflect::set(
            window.as_ref(),
            &JsValue::from_str("__logViewerSmokeImport"),
            import.as_ref(),
        )?;
        import.forget();
    }
    let document = window.document().unwrap();
    let canvas = document
        .get_element_by_id("app")
        .unwrap()
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| Ok(Box::new(LogViewer::new(cc)))),
        )
        .await
}
#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::JsCast;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);
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
    fn gauge_settings_round_trip() {
        let data = log::parse(include_bytes!("../tests/fixtures/sanitized.mlg")).unwrap();
        let mut gauge =
            ui::gauges::Gauge::from_field(&data, data.field_ids("Battery V")[0]).unwrap();
        gauge.size = 240.0;
        let settings = GaugeSettings {
            version: 1,
            initialized: true,
            gauges: vec![gauge],
            maps: ui::maps::MapPreferences {
                trail_seconds: 3.0,
                ..Default::default()
            },
            speed: 2.0,
            dark_theme: true,
            plots_tab: true,
        };
        let mut storage = MemoryStorage::default();
        eframe::set_value(&mut storage, "gauge-settings", &settings);
        let loaded: GaugeSettings = eframe::get_value(&storage, "gauge-settings").unwrap();
        assert_eq!(loaded.speed, 2.0);
        assert!(loaded.plots_tab);
        assert_eq!(loaded.maps.trail_seconds, 3.0);
        assert_eq!(loaded.gauges[0].size, 240.0);
        assert_eq!(loaded.gauges[0].critical_high, Some(16.0));
        assert_eq!(
            loaded.gauges[0].field_id(&data),
            Some(data.field_ids("Battery V")[0])
        );
    }
}
