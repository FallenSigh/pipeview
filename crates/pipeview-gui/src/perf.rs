use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tracing::info;

#[derive(Clone, Default)]
pub struct RepaintCounter {
    inner: Arc<AtomicU64>,
}

impl RepaintCounter {
    pub fn increment(&self) {
        self.inner.fetch_add(1, Ordering::Relaxed);
    }

    fn take(&self) -> u64 {
        self.inner.swap(0, Ordering::Relaxed)
    }
}

#[derive(Default)]
struct DurationStats {
    count: u64,
    total: Duration,
    max: Duration,
}

impl DurationStats {
    fn record(&mut self, elapsed: Duration) {
        self.count += 1;
        self.total += elapsed;
        self.max = self.max.max(elapsed);
    }

    fn avg_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            (self.total.as_secs_f64() * 1000.0) / self.count as f64
        }
    }

    fn max_ms(&self) -> f64 {
        self.max.as_secs_f64() * 1000.0
    }
}

#[derive(Default)]
struct IntervalStats {
    frames: DurationStats,
    drains: DurationStats,
    text_renders: DurationStats,
    hex_renders: DurationStats,
    plot_renders: DurationStats,
    drained_events: u64,
    drained_data_events: u64,
    drained_text_events: u64,
    drained_hex_events: u64,
    drained_plot_events: u64,
    max_pending_events: usize,
    max_text_lines: usize,
    max_hex_lines: usize,
    max_plot_series: usize,
    max_plot_stored_points: usize,
    max_plot_rendered_points: usize,
    tabs: usize,
    active_view: &'static str,
    active_text_lines: usize,
    active_hex_lines: usize,
    active_plot_series: usize,
    active_plot_points: usize,
}

pub struct DrainStats {
    pub drained_events: u64,
    pub drained_data_events: u64,
    pub drained_text_events: u64,
    pub drained_hex_events: u64,
    pub drained_plot_events: u64,
    pub pending_events: usize,
}

#[derive(Clone, Copy)]
pub struct GuiSnapshot {
    pub tabs: usize,
    pub active_view: &'static str,
    pub active_text_lines: usize,
    pub active_hex_lines: usize,
    pub active_plot_series: usize,
    pub active_plot_points: usize,
}

pub struct PlotRenderStats {
    pub series_count: usize,
    pub stored_points: usize,
    pub rendered_points: usize,
}

pub struct GuiProfiler {
    enabled: bool,
    interval: Duration,
    last_report_at: Instant,
    repaint_counter: RepaintCounter,
    interval_stats: IntervalStats,
}

impl GuiProfiler {
    pub fn from_env() -> Self {
        let enabled = env_flag("XSERIAL_GUI_PROFILE");
        let interval_ms = env::var("XSERIAL_GUI_PROFILE_INTERVAL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(1_000);
        Self {
            enabled,
            interval: Duration::from_millis(interval_ms),
            last_report_at: Instant::now(),
            repaint_counter: RepaintCounter::default(),
            interval_stats: IntervalStats {
                active_view: "none",
                ..IntervalStats::default()
            },
        }
    }

    pub fn repaint_counter(&self) -> RepaintCounter {
        self.repaint_counter.clone()
    }

    pub fn record_drain(&mut self, elapsed: Duration, stats: DrainStats) {
        if !self.enabled {
            return;
        }

        self.interval_stats.drains.record(elapsed);
        self.interval_stats.drained_events += stats.drained_events;
        self.interval_stats.drained_data_events += stats.drained_data_events;
        self.interval_stats.drained_text_events += stats.drained_text_events;
        self.interval_stats.drained_hex_events += stats.drained_hex_events;
        self.interval_stats.drained_plot_events += stats.drained_plot_events;
        self.interval_stats.max_pending_events = self
            .interval_stats
            .max_pending_events
            .max(stats.pending_events);
    }

    pub fn record_text_render(&mut self, elapsed: Duration, line_count: usize) {
        if !self.enabled {
            return;
        }

        self.interval_stats.text_renders.record(elapsed);
        self.interval_stats.max_text_lines = self.interval_stats.max_text_lines.max(line_count);
    }

    pub fn record_hex_render(&mut self, elapsed: Duration, line_count: usize) {
        if !self.enabled {
            return;
        }

        self.interval_stats.hex_renders.record(elapsed);
        self.interval_stats.max_hex_lines = self.interval_stats.max_hex_lines.max(line_count);
    }

    pub fn record_plot_render(&mut self, elapsed: Duration, stats: PlotRenderStats) {
        if !self.enabled {
            return;
        }

        self.interval_stats.plot_renders.record(elapsed);
        self.interval_stats.max_plot_series =
            self.interval_stats.max_plot_series.max(stats.series_count);
        self.interval_stats.max_plot_stored_points = self
            .interval_stats
            .max_plot_stored_points
            .max(stats.stored_points);
        self.interval_stats.max_plot_rendered_points = self
            .interval_stats
            .max_plot_rendered_points
            .max(stats.rendered_points);
    }

    pub fn record_frame(&mut self, elapsed: Duration, snapshot: GuiSnapshot) {
        if !self.enabled {
            return;
        }

        self.interval_stats.frames.record(elapsed);
        self.interval_stats.tabs = snapshot.tabs;
        self.interval_stats.active_view = snapshot.active_view;
        self.interval_stats.active_text_lines = snapshot.active_text_lines;
        self.interval_stats.active_hex_lines = snapshot.active_hex_lines;
        self.interval_stats.active_plot_series = snapshot.active_plot_series;
        self.interval_stats.active_plot_points = snapshot.active_plot_points;

        if self.last_report_at.elapsed() < self.interval {
            return;
        }

        let repaints = self.repaint_counter.take();
        info!(
            target: "pipeview_gui::perf",
            frames = self.interval_stats.frames.count,
            frame_avg_ms = self.interval_stats.frames.avg_ms(),
            frame_max_ms = self.interval_stats.frames.max_ms(),
            drains = self.interval_stats.drains.count,
            drain_avg_ms = self.interval_stats.drains.avg_ms(),
            drain_max_ms = self.interval_stats.drains.max_ms(),
            drained_events = self.interval_stats.drained_events,
            drained_data_events = self.interval_stats.drained_data_events,
            drained_text_events = self.interval_stats.drained_text_events,
            drained_hex_events = self.interval_stats.drained_hex_events,
            drained_plot_events = self.interval_stats.drained_plot_events,
            repaint_requests = repaints,
            max_pending_events = self.interval_stats.max_pending_events,
            text_renders = self.interval_stats.text_renders.count,
            text_avg_ms = self.interval_stats.text_renders.avg_ms(),
            text_max_ms = self.interval_stats.text_renders.max_ms(),
            max_text_lines = self.interval_stats.max_text_lines,
            hex_renders = self.interval_stats.hex_renders.count,
            hex_avg_ms = self.interval_stats.hex_renders.avg_ms(),
            hex_max_ms = self.interval_stats.hex_renders.max_ms(),
            max_hex_lines = self.interval_stats.max_hex_lines,
            plot_renders = self.interval_stats.plot_renders.count,
            plot_avg_ms = self.interval_stats.plot_renders.avg_ms(),
            plot_max_ms = self.interval_stats.plot_renders.max_ms(),
            max_plot_series = self.interval_stats.max_plot_series,
            max_plot_stored_points = self.interval_stats.max_plot_stored_points,
            max_plot_rendered_points = self.interval_stats.max_plot_rendered_points,
            tabs = self.interval_stats.tabs,
            active_view = self.interval_stats.active_view,
            active_text_lines = self.interval_stats.active_text_lines,
            active_hex_lines = self.interval_stats.active_hex_lines,
            active_plot_series = self.interval_stats.active_plot_series,
            active_plot_points = self.interval_stats.active_plot_points,
            "gui perf"
        );

        self.last_report_at = Instant::now();
        self.interval_stats = IntervalStats {
            active_view: "none",
            ..IntervalStats::default()
        };
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !value.is_empty() && value != "0" && value != "false" && value != "off"
        })
        .unwrap_or(false)
}
