use crate::{log::Dataset, playback::Playback};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};

#[derive(Clone, Copy)]
struct DragTarget(u8);

/// Full-width overview scrubber with independently draggable plot-range handles.
pub fn show(
    ui: &mut egui::Ui,
    data: &Dataset,
    playback: &mut Playback,
    range: &mut Option<[f64; 2]>,
) -> bool {
    let duration = data.duration();
    let mut changed_range = false;
    let mut selected = range.unwrap_or([0.0, duration]);
    if !selected[0].is_finite()
        || !selected[1].is_finite()
        || selected[0] < 0.0
        || selected[1] > duration
        || selected[0] >= selected[1]
    {
        selected = [0.0, duration];
        *range = None;
        changed_range = true;
    }

    let desired_width = ui.available_width().max(120.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(desired_width, 38.0), Sense::click_and_drag());
    let left = rect.left() + 8.0;
    let right = rect.right() - 8.0;
    let track = Rect::from_min_max(
        Pos2::new(left, rect.center().y - 3.0),
        Pos2::new(right.max(left + 1.0), rect.center().y + 3.0),
    );
    let time_x = |time: f64| {
        left + (right - left)
            * if duration > 0.0 {
                (time / duration).clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
    };
    let x_time = |x: f32| {
        if duration > 0.0 && right > left {
            ((x - left) / (right - left)).clamp(0.0, 1.0) as f64 * duration
        } else {
            0.0
        }
    };
    let start_x = time_x(selected[0]);
    let end_x = time_x(selected[1]);
    let play_x = time_x(playback.position);
    let painter = ui.painter();
    painter.rect_filled(track, 3.0, Color32::from_gray(48));
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(start_x.min(end_x), track.top()),
            Pos2::new(start_x.max(end_x), track.bottom()),
        ),
        2.0,
        Color32::from_rgb(43, 112, 145),
    );
    for x in [start_x, end_x] {
        painter.line_segment(
            [
                Pos2::new(x, track.top() - 7.0),
                Pos2::new(x, track.bottom() + 7.0),
            ],
            Stroke::new(2.0, Color32::from_rgb(112, 205, 230)),
        );
    }
    painter.line_segment(
        [
            Pos2::new(play_x, rect.top() + 2.0),
            Pos2::new(play_x, rect.bottom() - 2.0),
        ],
        Stroke::new(2.0, Color32::WHITE),
    );

    let drag_id = response.id.with("timeline-drag-target");
    if response.drag_started() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let x = pointer.x;
            let closest = [
                (0_u8, (x - play_x).abs()),
                (1_u8, (x - start_x).abs()),
                (2_u8, (x - end_x).abs()),
            ]
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1));
            let target = closest
                .filter(|(_, distance)| *distance <= 14.0)
                .map_or(0, |(target, _)| target);
            ui.ctx()
                .data_mut(|data| data.insert_temp(drag_id, DragTarget(target)));
        }
    }
    if response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let target = ui
                .ctx()
                .data(|data| data.get_temp::<DragTarget>(drag_id))
                .map_or(0, |target| target.0);
            let time = x_time(pointer.x);
            match target {
                1 => {
                    let minimum_gap = (duration / 2000.0).max(0.001);
                    selected[0] = time.min(selected[1] - minimum_gap).max(0.0);
                    changed_range = true;
                }
                2 => {
                    let minimum_gap = (duration / 2000.0).max(0.001);
                    selected[1] = time.max(selected[0] + minimum_gap).min(duration);
                    changed_range = true;
                }
                _ => playback.seek(time, data),
            }
        }
    }
    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            playback.seek(x_time(pointer.x), data);
        }
    }
    if changed_range {
        *range = Some(selected);
    }
    response.on_hover_text("Drag the white playhead to seek. Drag the blue handles to choose the plotted time interval.");

    ui.horizontal(|ui| {
        ui.label("Plot range");
        let start_changed = ui
            .add(
                egui::DragValue::new(&mut selected[0])
                    .range(0.0..=duration)
                    .speed(0.01)
                    .suffix(" s"),
            )
            .changed();
        ui.label("to");
        let end_changed = ui
            .add(
                egui::DragValue::new(&mut selected[1])
                    .range(0.0..=duration)
                    .speed(0.01)
                    .suffix(" s"),
            )
            .changed();
        if start_changed || end_changed {
            let minimum_gap = (duration / 2000.0).max(0.001).min(duration);
            if selected[0] >= selected[1] {
                if start_changed {
                    selected[1] = (selected[0] + minimum_gap).min(duration);
                    selected[0] = selected[0].min(selected[1] - minimum_gap);
                } else {
                    selected[0] = (selected[1] - minimum_gap).max(0.0);
                    selected[1] = selected[1].max(selected[0] + minimum_gap);
                }
            }
        }
        ui.label(format!(
            "· playhead {:.3} / {:.3} s",
            playback.position, duration
        ));
        if let Some(index) = playback.sample_index {
            let sample_time = data.elapsed[index];
            if (sample_time - playback.position).abs() > 0.000_001 {
                ui.label(format!("· recorded sample {sample_time:.3} s"));
            }
        }
        let can_step = data.elapsed.len() > 1;
        if ui
            .add_enabled(can_step, egui::Button::new("‹ Sample"))
            .clicked()
        {
            playback.step_sample(-1, data);
        }
        if ui
            .add_enabled(can_step, egui::Button::new("Sample ›"))
            .clicked()
        {
            playback.step_sample(1, data);
        }
        if ui.button("Full log").clicked() {
            selected = [0.0, duration];
            *range = None;
            changed_range = true;
        }
    });
    if selected[1] > selected[0] && selected != range.unwrap_or([0.0, duration]) {
        *range = Some(selected);
        changed_range = true;
    }
    changed_range
}
