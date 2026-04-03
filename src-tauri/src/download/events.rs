use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::types::{SegmentStatus, TaskStatus};

/// Progress tick event — emitted every 500ms during download (per EV-01)
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressEvent {
    pub task_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    /// Smoothed download speed in bytes/sec (per EV-02)
    pub speed: f64,
    /// Estimated time remaining in seconds, None if speed is zero
    pub eta_secs: Option<f64>,
}

/// Segment status change event — emitted immediately (per EV-03, D-06)
#[derive(Debug, Clone, Serialize)]
pub struct DownloadSegmentEvent {
    pub task_id: String,
    pub segment_index: u16,
    pub status: SegmentStatus,
    pub downloaded: u64,
}

/// Task status change event — emitted immediately (per EV-04, D-07)
#[derive(Debug, Clone, Serialize)]
pub struct DownloadTaskEvent {
    pub task_id: String,
    pub status: TaskStatus,
}

/// EMA-based speed calculator (per D-01, EV-02)
pub struct SpeedCalculator {
    alpha: f64,
    smoothed_speed: f64,
    last_bytes: u64,
    last_time: std::time::Instant,
}

impl SpeedCalculator {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            smoothed_speed: 0.0,
            last_bytes: 0,
            last_time: std::time::Instant::now(),
        }
    }

    /// Update with current total bytes downloaded, returns smoothed speed (bytes/sec).
    pub fn update(&mut self, current_bytes: u64) -> f64 {
        let elapsed = self.last_time.elapsed().as_secs_f64();

        // Avoid division by near-zero elapsed time
        if elapsed < 0.001 {
            return self.smoothed_speed;
        }

        let delta_bytes = current_bytes.saturating_sub(self.last_bytes);
        let raw_speed = delta_bytes as f64 / elapsed;

        if self.smoothed_speed == 0.0 {
            // First update: set directly to avoid slow ramp-up from zero
            self.smoothed_speed = raw_speed;
        } else {
            self.smoothed_speed = self.alpha * raw_speed + (1.0 - self.alpha) * self.smoothed_speed;
        }

        self.last_bytes = current_bytes;
        self.last_time = std::time::Instant::now();

        self.smoothed_speed
    }

    /// Current smoothed speed in bytes/sec.
    #[allow(dead_code)]
    pub fn speed(&self) -> f64 {
        self.smoothed_speed
    }

    /// Estimated time remaining in seconds, None if speed is zero (per D-02).
    pub fn eta(&self, remaining_bytes: u64) -> Option<f64> {
        if self.smoothed_speed > 0.0 {
            Some(remaining_bytes as f64 / self.smoothed_speed)
        } else {
            None
        }
    }
}

pub fn emit_progress(app: &AppHandle, event: &DownloadProgressEvent) {
    let _ = app.emit("download:progress", event);
}

pub fn emit_segment_status(app: &AppHandle, event: &DownloadSegmentEvent) {
    let _ = app.emit("download:segment-status", event);
}

pub fn emit_task_status(app: &AppHandle, event: &DownloadTaskEvent) {
    let _ = app.emit("download:task-status", event);
}

/// URL expiration event — frontend should fetch a new URL (per URL-02)
#[derive(Debug, Clone, Serialize)]
pub struct UrlExpiredEvent {
    pub task_id: String,
    pub pick_code: String,
}

pub fn emit_url_expired(app: &AppHandle, event: &UrlExpiredEvent) {
    let _ = app.emit("download:url-expired", event);
}
