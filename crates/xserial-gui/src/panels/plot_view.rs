use crate::buffers::{PlotBuffer, PlotSeriesKind};
use crate::perf::PlotRenderStats;
use egui::{Button, Ui, vec2};
use egui_plot::{Legend, Line, Plot, PlotBounds, PlotPoints};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlotDisplayMode {
    Embedded,
    Detached,
}

pub struct PlotViewState {
    pub lock_x: bool,
    pub lock_y: bool,
    pub box_zoom: bool,
    pub follow_latest: bool,
    pub last_bounds: Option<PlotBounds>,
    pub detached: bool,
}

impl Default for PlotViewState {
    fn default() -> Self {
        Self {
            lock_x: false,
            lock_y: false,
            box_zoom: false,
            follow_latest: false,
            last_bounds: None,
            detached: false,
        }
    }
}

pub struct PlotRenderOutput {
    pub stats: PlotRenderStats,
    pub toggle_detached: bool,
}

pub fn render(
    ui: &mut Ui,
    buf: &mut PlotBuffer,
    session_id: u64,
    state: &mut PlotViewState,
) -> PlotRenderOutput {
    let mode = if state.detached {
        PlotDisplayMode::Detached
    } else {
        PlotDisplayMode::Embedded
    };
    let mut toggle_detached = false;

    if buf.is_empty() {
        ui.horizontal_wrapped(|ui| {
            let toggle_label = match mode {
                PlotDisplayMode::Embedded => "Pop Out",
                PlotDisplayMode::Detached => "Dock Back",
            };
            if ui.button(toggle_label).clicked() {
                toggle_detached = true;
            }
            ui.label("no plot data");
        });
        return PlotRenderOutput {
            stats: PlotRenderStats {
                series_count: 0,
                stored_points: 0,
                rendered_points: 0,
            },
            toggle_detached,
        };
    }

    let mut auto_fit = false;
    let mut reset_view = false;
    let mut zoom_x = 1.0_f32;
    let mut zoom_y = 1.0_f32;

    ui.horizontal_wrapped(|ui| {
        if ui.button("Auto Fit").clicked() {
            auto_fit = true;
        }
        if ui.button("Reset View").clicked() {
            reset_view = true;
        }
        let toggle_label = match mode {
            PlotDisplayMode::Embedded => "Pop Out",
            PlotDisplayMode::Detached => "Dock Back",
        };
        if ui.button(toggle_label).clicked() {
            toggle_detached = true;
        }
        ui.separator();
        ui.checkbox(&mut state.lock_x, "Lock X");
        ui.checkbox(&mut state.lock_y, "Lock Y");
        ui.checkbox(&mut state.box_zoom, "Box Zoom");
        if matches!(buf.kind(), PlotSeriesKind::TimeSeries) {
            ui.checkbox(&mut state.follow_latest, "Follow Latest");
        } else {
            state.follow_latest = false;
        }
        ui.separator();
        if ui
            .add_enabled(!state.lock_x, Button::new("X-"))
            .on_hover_text("Zoom out X axis")
            .clicked()
        {
            zoom_x = 0.8;
        }
        if ui
            .add_enabled(!state.lock_x, Button::new("X+"))
            .on_hover_text("Zoom in X axis")
            .clicked()
        {
            zoom_x = 1.25;
        }
        if ui
            .add_enabled(!state.lock_y, Button::new("Y-"))
            .on_hover_text("Zoom out Y axis")
            .clicked()
        {
            zoom_y = 0.8;
        }
        if ui
            .add_enabled(!state.lock_y, Button::new("Y+"))
            .on_hover_text("Zoom in Y axis")
            .clicked()
        {
            zoom_y = 1.25;
        }
    });

    if let Some(bounds) = state.last_bounds {
        ui.horizontal_wrapped(|ui| {
            ui.monospace(format!(
                "X [{:.3}, {:.3}]  Y [{:.3}, {:.3}]",
                bounds.min()[0],
                bounds.max()[0],
                bounds.min()[1],
                bounds.max()[1],
            ));
        });
    }

    let data_bounds = buf.plot_bounds();
    let mut plot = Plot::new(format!("plot_view_{session_id}"))
        .legend(Legend::default())
        .allow_boxed_zoom(state.box_zoom)
        .allow_zoom([!state.lock_x, !state.lock_y])
        .allow_drag([!state.lock_x, !state.lock_y])
        .allow_axis_zoom_drag([!state.lock_x, !state.lock_y])
        .allow_scroll([
            !state.lock_x && matches!(buf.kind(), PlotSeriesKind::TimeSeries),
            !state.lock_y && matches!(buf.kind(), PlotSeriesKind::TimeSeries),
        ]);
    if reset_view {
        plot = plot.reset();
    }

    let mut rendered_points = 0usize;
    plot.show(ui, |plot_ui| {
        let current_bounds = plot_ui.plot_bounds();
        state.last_bounds = current_bounds.is_finite().then_some(current_bounds);
        let pixel_width = plot_ui.response().rect.width().max(64.0);
        let max_render_points = (pixel_width as usize).saturating_mul(2);

        if auto_fit {
            if let Some(bounds) = data_bounds {
                plot_ui.set_auto_bounds(false);
                plot_ui.set_plot_bounds(bounds);
                state.last_bounds = Some(bounds);
            }
        } else if state.follow_latest && matches!(buf.kind(), PlotSeriesKind::TimeSeries) {
            if let Some(bounds) = data_bounds {
                let current_width = current_bounds.width();
                let data_width = bounds.width();
                let x_range = if current_width.is_finite()
                    && current_width > 0.0
                    && data_width > current_width
                {
                    (bounds.max()[0] - current_width)..=bounds.max()[0]
                } else {
                    bounds.min()[0]..=bounds.max()[0]
                };
                plot_ui.set_auto_bounds([false, plot_ui.auto_bounds().y]);
                plot_ui.set_plot_bounds_x(x_range.clone());

                let mut followed = current_bounds;
                followed.set_x(&PlotBounds::from_min_max(
                    [*x_range.start(), current_bounds.min()[1]],
                    [*x_range.end(), current_bounds.max()[1]],
                ));
                state.last_bounds = Some(followed);
            }
        } else if zoom_x != 1.0 || zoom_y != 1.0 {
            let center = plot_ui.plot_bounds().center();
            plot_ui.zoom_bounds(vec2(zoom_x, zoom_y), center);
        }

        if let Some(bounds) = data_bounds {
            if !bounds.is_valid_x() {
                plot_ui.set_auto_bounds([true, plot_ui.auto_bounds().y]);
            }
            if !bounds.is_valid_y() {
                plot_ui.set_auto_bounds([plot_ui.auto_bounds().x, true]);
            }
        }

        for series in buf.iter() {
            let sampled_points = match buf.kind() {
                PlotSeriesKind::TimeSeries => series.render_points_time_series(
                    current_bounds.min()[0],
                    current_bounds.max()[0],
                    max_render_points,
                ),
                PlotSeriesKind::XY => series.render_points_xy(max_render_points),
            };
            rendered_points += sampled_points.len();
            let points = PlotPoints::from_iter(sampled_points);
            plot_ui.line(Line::new(series.name.clone(), points));
        }
    });

    PlotRenderOutput {
        stats: PlotRenderStats {
            series_count: buf.series_len(),
            stored_points: buf.total_points(),
            rendered_points,
        },
        toggle_detached,
    }
}
