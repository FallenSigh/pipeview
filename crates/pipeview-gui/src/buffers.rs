use egui_plot::PlotBounds;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use pipeview_client::RingBuffer;
use pipeview_client::event::DecodedEntry;
use pipeview_core::protocol::DecodedData;
use pipeview_core::protocol::plot::{PlotFormat, PlotFrame};

#[derive(Clone)]
pub enum LineDirection {
    In,
    Out,
}

#[derive(Clone)]
pub struct ConsoleLine {
    pub elapsed: Duration,
    pub pipeline: String,
    pub text: String,
    pub direction: LineDirection,
}

pub struct TextBuffer {
    started_at: Instant,
    lines: RingBuffer<ConsoleLine>,
}

impl TextBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            started_at: Instant::now(),
            lines: RingBuffer::new(cap),
        }
    }
    pub fn push(&mut self, entry: &DecodedEntry) {
        if let DecodedData::Text(s) = &entry.data {
            self.lines.push(ConsoleLine {
                elapsed: self.started_at.elapsed(),
                pipeline: entry.pipeline_name.clone(),
                text: s.clone(),
                direction: LineDirection::In,
            });
        }
    }

    pub fn push_outbound(&mut self, text: String) {
        self.lines.push(ConsoleLine {
            elapsed: self.started_at.elapsed(),
            pipeline: String::from("OUT"),
            text,
            direction: LineDirection::Out,
        });
    }

    pub fn get(&self, index: usize) -> Option<&ConsoleLine> {
        self.lines.get(index)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.lines.set_limit(limit);
    }

    // pub fn iter(&self) -> impl Iterator<Item = &ConsoleLine> {
    //     (0..self.lines.len()).filter_map(|i| self.lines.get(i))
    // }

    pub fn search(&self, query: &str, case_sensitive: bool) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = if case_sensitive {
            String::new()
        } else {
            query.to_lowercase()
        };
        (0..self.lines.len())
            .filter(|&i| {
                self.lines.get(i).is_some_and(|line| {
                    if case_sensitive {
                        line.text.contains(query)
                    } else {
                        line.text.to_lowercase().contains(&query_lower)
                    }
                })
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct HexLine {
    pub elapsed: Duration,
    pub pipeline: String,
    pub hex: String,
    pub ascii: String,
    pub direction: LineDirection,
}

pub struct HexBuffer {
    started_at: Instant,
    lines: RingBuffer<HexLine>,
}

impl HexBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            started_at: Instant::now(),
            lines: RingBuffer::new(cap),
        }
    }
    pub fn push(&mut self, entry: &DecodedEntry) {
        if let DecodedData::Hex(s) = &entry.data {
            let ascii = hex::decode(s.replace(' ', ""))
                .map(|bytes| {
                    bytes
                        .into_iter()
                        .map(|b| {
                            if b.is_ascii_graphic() || b == b' ' {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect()
                })
                .unwrap_or_else(|_| String::from("[invalid hex]"));
            self.lines.push(HexLine {
                elapsed: self.started_at.elapsed(),
                pipeline: entry.pipeline_name.clone(),
                hex: s.clone(),
                ascii,
                direction: LineDirection::In,
            });
        }
    }

    pub fn push_outbound(&mut self, hex: String) {
        let ascii = hex::decode(hex.replace(' ', ""))
            .map(|bytes| {
                bytes
                    .into_iter()
                    .map(|b| {
                        if b.is_ascii_graphic() || b == b' ' {
                            b as char
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|_| String::from("[invalid hex]"));
        self.lines.push(HexLine {
            elapsed: self.started_at.elapsed(),
            pipeline: String::from("OUT"),
            hex,
            ascii,
            direction: LineDirection::Out,
        });
    }

    pub fn get(&self, index: usize) -> Option<&HexLine> {
        self.lines.get(index)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.lines.set_limit(limit);
    }

    // pub fn iter(&self) -> impl Iterator<Item = &HexLine> {
    //     (0..self.lines.len()).filter_map(|i| self.lines.get(i))
    // }

    pub fn search(&self, query: &str, case_sensitive: bool) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = if case_sensitive {
            String::new()
        } else {
            query.to_lowercase()
        };
        (0..self.lines.len())
            .filter(|&i| {
                self.lines.get(i).is_some_and(|line| {
                    if case_sensitive {
                        line.hex.contains(query) || line.ascii.contains(query)
                    } else {
                        line.hex.to_lowercase().contains(&query_lower)
                            || line.ascii.to_lowercase().contains(&query_lower)
                    }
                })
            })
            .collect()
    }
}

pub struct PlotSeries {
    pub name: String,
    points: VecDeque<[f64; 2]>,
    next_x: f64,
}

impl PlotSeries {
    fn new(name: String) -> Self {
        Self {
            name,
            points: VecDeque::new(),
            next_x: 0.0,
        }
    }

    fn push_samples(&mut self, samples: &[f64], limit: usize) {
        for sample in samples {
            if !sample.is_finite() {
                continue;
            }
            self.push_point([self.next_x, *sample], limit);
            self.next_x += 1.0;
        }
    }

    fn push_point(&mut self, point: [f64; 2], limit: usize) -> Option<[f64; 2]> {
        self.points.push_back(point);
        if self.points.len() > limit {
            self.points.pop_front()
        } else {
            None
        }
    }

    pub fn points(&self) -> impl Iterator<Item = [f64; 2]> + '_ {
        self.points.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    fn first_point(&self) -> Option<[f64; 2]> {
        self.points.front().copied()
    }

    fn last_point(&self) -> Option<[f64; 2]> {
        self.points.back().copied()
    }

    pub fn render_points_time_series(
        &self,
        x_min: f64,
        x_max: f64,
        max_points: usize,
    ) -> Vec<[f64; 2]> {
        if self.points.is_empty() || max_points == 0 {
            return Vec::new();
        }

        let mut exact_visible = Vec::with_capacity(max_points.min(self.points.len()));
        for point in self.points.iter().copied() {
            if point[0] < x_min {
                continue;
            }
            if point[0] > x_max {
                break;
            }
            exact_visible.push(point);
            if exact_visible.len() > max_points {
                exact_visible.clear();
                break;
            }
        }
        if !exact_visible.is_empty() {
            return exact_visible;
        }

        let bucket_count = (max_points / 2).max(1);
        let width = (x_max - x_min).max(1.0);
        let bucket_width = width / bucket_count as f64;
        let mut rendered = Vec::with_capacity(bucket_count * 2);
        let mut points = self
            .points
            .iter()
            .copied()
            .skip_while(|point| point[0] < x_min)
            .peekable();

        for bucket_index in 0..bucket_count {
            let bucket_start = x_min + bucket_width * bucket_index as f64;
            let bucket_end = if bucket_index + 1 == bucket_count {
                x_max
            } else {
                bucket_start + bucket_width
            };

            let mut min_point: Option<[f64; 2]> = None;
            let mut max_point: Option<[f64; 2]> = None;

            while let Some(point) = points.peek().copied() {
                if point[0] > bucket_end {
                    break;
                }
                if point[0] >= bucket_start {
                    match min_point {
                        Some(current) if current[1] <= point[1] => {}
                        _ => min_point = Some(point),
                    }
                    match max_point {
                        Some(current) if current[1] >= point[1] => {}
                        _ => max_point = Some(point),
                    }
                }
                points.next();
            }

            match (min_point, max_point) {
                (Some(a), Some(b)) if a[0] <= b[0] => {
                    rendered.push(a);
                    if a != b {
                        rendered.push(b);
                    }
                }
                (Some(a), Some(b)) => {
                    rendered.push(b);
                    if a != b {
                        rendered.push(a);
                    }
                }
                (Some(a), None) | (None, Some(a)) => rendered.push(a),
                (None, None) => {}
            }
        }

        if rendered.len() > max_points {
            let stride = rendered.len().div_ceil(max_points);
            rendered.into_iter().step_by(stride).collect()
        } else {
            rendered
        }
    }

    pub fn render_points_xy(&self, max_points: usize) -> Vec<[f64; 2]> {
        if self.points.is_empty() || max_points == 0 {
            return Vec::new();
        }
        if self.points.len() <= max_points {
            return self.points.iter().copied().collect();
        }

        let stride = self.points.len().div_ceil(max_points);
        self.points.iter().copied().step_by(stride).collect()
    }
}

pub enum PlotSeriesKind {
    TimeSeries,
    XY,
}

#[derive(Clone, Copy)]
struct BoundsRect {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl BoundsRect {
    fn from_point([x, y]: [f64; 2]) -> Option<Self> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        Some(Self {
            min_x: x,
            max_x: x,
            min_y: y,
            max_y: y,
        })
    }

    fn extend_with_point(&mut self, [x, y]: [f64; 2]) {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }

    fn touches(&self, [x, y]: [f64; 2]) -> bool {
        x == self.min_x || x == self.max_x || y == self.min_y || y == self.max_y
    }

    fn into_plot_bounds(self) -> PlotBounds {
        let mut bounds =
            PlotBounds::from_min_max([self.min_x, self.min_y], [self.max_x, self.max_y]);

        if bounds.width() <= 0.0 {
            bounds.set_x_center_width(bounds.center().x, 1.0);
        } else {
            bounds.expand_x(bounds.width() * 0.05);
        }

        if bounds.height() <= 0.0 {
            bounds.set_y_center_height(bounds.center().y, 1.0);
        } else {
            bounds.expand_y(bounds.height() * 0.1);
        }

        bounds
    }
}

pub struct PlotBuffer {
    limit: usize,
    kind: PlotSeriesKind,
    series: Vec<PlotSeries>,
    time_series_y_bounds: Option<(f64, f64)>,
    time_series_y_dirty: bool,
    xy_bounds: Option<BoundsRect>,
    xy_bounds_dirty: bool,
}

impl PlotBuffer {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            kind: PlotSeriesKind::TimeSeries,
            series: Vec::new(),
            time_series_y_bounds: None,
            time_series_y_dirty: false,
            xy_bounds: None,
            xy_bounds_dirty: false,
        }
    }

    pub fn push(&mut self, entry: &DecodedEntry) {
        if let DecodedData::Plot(frame) = &entry.data {
            self.push_frame(&entry.pipeline_name, frame);
        }
    }

    fn push_frame(&mut self, pipeline_name: &str, frame: &PlotFrame) {
        let next_kind = match frame.format {
            PlotFormat::XY => PlotSeriesKind::XY,
            PlotFormat::Interleaved | PlotFormat::Block => PlotSeriesKind::TimeSeries,
        };
        if !matches!(
            (&self.kind, &next_kind),
            (PlotSeriesKind::TimeSeries, PlotSeriesKind::TimeSeries)
                | (PlotSeriesKind::XY, PlotSeriesKind::XY)
        ) {
            self.invalidate_bounds();
        }
        self.kind = next_kind;

        if matches!(frame.format, PlotFormat::XY) {
            self.push_xy_frame(pipeline_name, frame);
            return;
        }

        for (index, channel) in frame.channels.iter().enumerate() {
            let series_name = if frame.channels.len() == 1 {
                pipeline_name.to_owned()
            } else {
                format!("{pipeline_name}:ch{}", index + 1)
            };

            if let Some(series_index) = self
                .series
                .iter()
                .position(|series| series.name == series_name)
            {
                for sample in channel {
                    if !sample.is_finite() {
                        continue;
                    }
                    let (point, removed) = {
                        let series = &mut self.series[series_index];
                        let point = [series.next_x, *sample];
                        let removed = series.push_point(point, self.limit);
                        series.next_x += 1.0;
                        (point, removed)
                    };
                    if let Some(removed) = removed {
                        self.note_time_series_removed(removed);
                    }
                    self.note_time_series_point(point);
                }
            } else {
                let mut series = PlotSeries::new(series_name);
                series.push_samples(channel, self.limit);
                for point in series.points() {
                    self.note_time_series_point(point);
                }
                self.series.push(series);
            }
        }
    }

    fn push_xy_frame(&mut self, pipeline_name: &str, frame: &PlotFrame) {
        if frame.channels.len() < 2 {
            return;
        }

        let x = &frame.channels[0];
        let y = &frame.channels[1];
        let len = x.len().min(y.len());
        let series_name = format!("{pipeline_name}:xy");

        let series_index = if let Some(index) = self
            .series
            .iter()
            .position(|series| series.name == series_name)
        {
            index
        } else {
            self.series.push(PlotSeries::new(series_name));
            self.series.len() - 1
        };

        for index in 0..len {
            if !x[index].is_finite() || !y[index].is_finite() {
                continue;
            }
            let point = [x[index], y[index]];
            let removed = {
                let series = &mut self.series[series_index];
                series.push_point(point, self.limit)
            };
            if let Some(removed) = removed {
                self.note_xy_removed(removed);
            }
            self.note_xy_point(point);
        }
    }

    pub fn clear(&mut self) {
        self.series.clear();
        self.invalidate_bounds();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        for series in &mut self.series {
            while series.points.len() > self.limit {
                series.points.pop_front();
            }
        }
        self.invalidate_bounds();
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    pub fn kind(&self) -> &PlotSeriesKind {
        &self.kind
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlotSeries> {
        self.series.iter()
    }

    pub fn series_len(&self) -> usize {
        self.series.len()
    }

    pub fn total_points(&self) -> usize {
        self.series.iter().map(PlotSeries::len).sum()
    }

    pub fn plot_bounds(&mut self) -> Option<PlotBounds> {
        match self.kind {
            PlotSeriesKind::TimeSeries => self.time_series_plot_bounds(),
            PlotSeriesKind::XY => self.xy_plot_bounds(),
        }
    }

    fn invalidate_bounds(&mut self) {
        self.time_series_y_bounds = None;
        self.time_series_y_dirty = false;
        self.xy_bounds = None;
        self.xy_bounds_dirty = false;
    }

    fn note_time_series_point(&mut self, point: [f64; 2]) {
        if !point[1].is_finite() {
            return;
        }
        match &mut self.time_series_y_bounds {
            Some((min_y, max_y)) => {
                *min_y = min_y.min(point[1]);
                *max_y = max_y.max(point[1]);
            }
            None => self.time_series_y_bounds = Some((point[1], point[1])),
        }
    }

    fn note_time_series_removed(&mut self, removed: [f64; 2]) {
        if let Some((min_y, max_y)) = self.time_series_y_bounds
            && (removed[1] == min_y || removed[1] == max_y)
        {
            self.time_series_y_dirty = true;
        }
    }

    fn note_xy_point(&mut self, point: [f64; 2]) {
        match &mut self.xy_bounds {
            Some(bounds) => bounds.extend_with_point(point),
            None => self.xy_bounds = BoundsRect::from_point(point),
        }
    }

    fn note_xy_removed(&mut self, removed: [f64; 2]) {
        if let Some(bounds) = self.xy_bounds
            && bounds.touches(removed)
        {
            self.xy_bounds_dirty = true;
        }
    }

    fn time_series_plot_bounds(&mut self) -> Option<PlotBounds> {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;

        for series in &self.series {
            if let Some([x, _]) = series.first_point() {
                min_x = min_x.min(x);
            }
            if let Some([x, _]) = series.last_point() {
                max_x = max_x.max(x);
            }
        }

        if !(min_x.is_finite() && max_x.is_finite()) {
            return None;
        }

        if self.time_series_y_dirty {
            self.recompute_time_series_y_bounds();
        }
        let (min_y, max_y) = self.time_series_y_bounds?;

        Some(
            BoundsRect {
                min_x,
                max_x,
                min_y,
                max_y,
            }
            .into_plot_bounds(),
        )
    }

    fn xy_plot_bounds(&mut self) -> Option<PlotBounds> {
        if self.xy_bounds_dirty {
            self.recompute_xy_bounds();
        }
        self.xy_bounds.map(BoundsRect::into_plot_bounds)
    }

    fn recompute_time_series_y_bounds(&mut self) {
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for series in &self.series {
            for [_, y] in series.points() {
                if y.is_finite() {
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }

        self.time_series_y_bounds = if min_y.is_finite() && max_y.is_finite() {
            Some((min_y, max_y))
        } else {
            None
        };
        self.time_series_y_dirty = false;
    }

    fn recompute_xy_bounds(&mut self) {
        let mut bounds: Option<BoundsRect> = None;

        for series in &self.series {
            for point in series.points() {
                match &mut bounds {
                    Some(existing) => existing.extend_with_point(point),
                    None => bounds = BoundsRect::from_point(point),
                }
            }
        }

        self.xy_bounds = bounds;
        self.xy_bounds_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipeview_client::event::DecodedEntry;
    use pipeview_core::protocol::DecodedData;
    use pipeview_core::protocol::plot::{PlotFrame, SampleType};

    fn plot_entry() -> DecodedEntry {
        DecodedEntry {
            pipeline_name: String::from("plot"),
            data: DecodedData::Plot(PlotFrame {
                channels: vec![vec![1.0, 2.0, 3.0], vec![10.0, 20.0, 30.0]],
                raw: vec![],
                sample_type: SampleType::F32,
                format: PlotFormat::Interleaved,
            }),
        }
    }

    fn xy_plot_entry() -> DecodedEntry {
        DecodedEntry {
            pipeline_name: String::from("xy"),
            data: DecodedData::Plot(PlotFrame {
                channels: vec![vec![0.0, 1.0, 2.0], vec![10.0, 20.0, 30.0]],
                raw: vec![],
                sample_type: SampleType::F32,
                format: PlotFormat::XY,
            }),
        }
    }

    #[test]
    fn plot_buffer_creates_series_per_channel() {
        let mut buffer = PlotBuffer::new(8);
        buffer.push(&plot_entry());

        let series: Vec<_> = buffer.iter().collect();
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].name, "plot:ch1");
        assert_eq!(series[1].name, "plot:ch2");
        assert_eq!(series[0].points().count(), 3);
        assert_eq!(series[1].points().count(), 3);
    }

    #[test]
    fn plot_buffer_limit_and_clear_work() {
        let mut buffer = PlotBuffer::new(2);
        buffer.push(&plot_entry());
        assert_eq!(buffer.iter().next().unwrap().points().count(), 2);

        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn plot_buffer_xy_uses_xy_points() {
        let mut buffer = PlotBuffer::new(8);
        buffer.push(&xy_plot_entry());

        assert!(matches!(buffer.kind(), PlotSeriesKind::XY));
        let series: Vec<_> = buffer.iter().collect();
        assert_eq!(series.len(), 1);
        let points: Vec<_> = series[0].points().collect();
        assert_eq!(points, vec![[0.0, 10.0], [1.0, 20.0], [2.0, 30.0]]);
    }

    #[test]
    fn plot_series_time_series_downsamples_to_budget() {
        let mut series = PlotSeries::new(String::from("plot"));
        series.push_samples(&(0..100).map(|n| n as f64).collect::<Vec<_>>(), 200);

        let rendered = series.render_points_time_series(0.0, 99.0, 16);
        assert!(!rendered.is_empty());
        assert!(rendered.len() <= 16);
        assert!(
            rendered
                .iter()
                .all(|point| point[0] >= 0.0 && point[0] <= 99.0)
        );
    }

    #[test]
    fn plot_series_xy_downsamples_to_budget() {
        let mut series = PlotSeries::new(String::from("xy"));
        for n in 0..100 {
            series.points.push_back([n as f64, (n * 2) as f64]);
        }

        let rendered = series.render_points_xy(10);
        assert!(rendered.len() <= 10);
        assert_eq!(rendered.first().copied(), Some([0.0, 0.0]));
    }
}
