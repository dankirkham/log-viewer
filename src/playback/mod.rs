use crate::log::Dataset;

pub const SPEEDS: [f64; 7] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

#[derive(Debug)]
pub struct Playback {
    pub position: f64,
    pub playing: bool,
    pub speed: f64,
    pub sample_index: Option<usize>,
    last_tick: Option<f64>,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            position: 0.0,
            playing: false,
            speed: 1.0,
            sample_index: None,
            last_tick: None,
        }
    }
}
impl Playback {
    pub fn reset(&mut self, data: &Dataset) {
        self.position = 0.0;
        self.playing = false;
        self.last_tick = None;
        self.sample_index = (!data.timestamps.is_empty()).then_some(0);
    }
    pub fn seek(&mut self, position: f64, data: &Dataset) {
        if !position.is_finite() {
            return;
        }
        self.position = position.clamp(0.0, data.duration());
        self.sample_index = data.sample_at(self.position);
        self.last_tick = None;
    }
    pub fn toggle(&mut self, data: &Dataset) {
        if self.playing {
            self.playing = false;
        } else if data.timestamps.len() > 1 && data.duration() > 0.0 {
            if self.position >= data.duration() {
                self.seek(0.0, data);
            }
            self.playing = true;
        }
        self.last_tick = None;
    }
    pub fn set_speed(&mut self, speed: f64) {
        if SPEEDS.contains(&speed) {
            self.speed = speed;
        }
    }
    /// Advances from monotonic seconds supplied by the host, independently of frame rate.
    pub fn tick(&mut self, now: f64, data: &Dataset) {
        if !self.playing || !now.is_finite() {
            self.last_tick = None;
            return;
        }
        if let Some(last) = self.last_tick {
            let delta = (now - last).max(0.0);
            self.position = (self.position + delta * self.speed).min(data.duration());
            self.sample_index = data.sample_at(self.position);
            if self.position >= data.duration() {
                self.playing = false;
                self.last_tick = None;
                return;
            }
        }
        self.last_tick = Some(now);
    }
}
