use std::collections::VecDeque;
use std::time::{Duration, Instant};
use xserial_client::RingBuffer;
use xserial_client::event::DecodedEntry;
use xserial_core::protocol::DecodedData;
use xserial_core::protocol::plot::{PlotFormat, PlotFrame};

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

    pub fn iter(&self) -> impl Iterator<Item = &ConsoleLine> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.lines.set_limit(limit);
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

    pub fn iter(&self) -> impl Iterator<Item = &HexLine> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.lines.set_limit(limit);
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
            self.points.push_back([self.next_x, *sample]);
            self.next_x += 1.0;
        }
        while self.points.len() > limit {
            self.points.pop_front();
        }
    }

    pub fn points(&self) -> impl Iterator<Item = [f64; 2]> + '_ {
        self.points.iter().copied()
    }

    pub fn render_points_time_series(&self, x_min: f64, x_max: f64, max_points: usize) -> Vec<[f64; 2]> {
        if self.points.is_empty() || max_points == 0 {
            return Vec::new();
        }

        let visible: Vec<[f64; 2]> = self
            .points
            .iter()
            .copied()
            .filter(|point| point[0] >= x_min && point[0] <= x_max)
            .collect();
        if visible.len() <= max_points {
            return visible;
        }

        let bucket_count = (max_points / 2).max(1);
        let width = (x_max - x_min).max(1.0);
        let bucket_width = width / bucket_count as f64;
        let mut rendered = Vec::with_capacity(bucket_count * 2);
        let mut start = 0usize;

        for bucket_index in 0..bucket_count {
            let bucket_start = x_min + bucket_width * bucket_index as f64;
            let bucket_end = if bucket_index + 1 == bucket_count {
                x_max
            } else {
                bucket_start + bucket_width
            };

            let mut min_point: Option<[f64; 2]> = None;
            let mut max_point: Option<[f64; 2]> = None;

            while start < visible.len() {
                let point = visible[start];
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
                start += 1;
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

pub struct PlotBuffer {
    limit: usize,
    kind: PlotSeriesKind,
    series: Vec<PlotSeries>,
}

impl PlotBuffer {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            kind: PlotSeriesKind::TimeSeries,
            series: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: &DecodedEntry) {
        if let DecodedData::Plot(frame) = &entry.data {
            self.push_frame(&entry.pipeline_name, frame);
        }
    }

    fn push_frame(&mut self, pipeline_name: &str, frame: &PlotFrame) {
        self.kind = match frame.format {
            PlotFormat::XY => PlotSeriesKind::XY,
            PlotFormat::Interleaved | PlotFormat::Block => PlotSeriesKind::TimeSeries,
        };

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

            if let Some(series) = self
                .series
                .iter_mut()
                .find(|series| series.name == series_name)
            {
                series.push_samples(channel, self.limit);
            } else {
                let mut series = PlotSeries::new(series_name);
                series.push_samples(channel, self.limit);
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

        let series = if let Some(series) = self
            .series
            .iter_mut()
            .find(|series| series.name == series_name)
        {
            series
        } else {
            self.series.push(PlotSeries::new(series_name));
            self.series.last_mut().expect("just pushed")
        };

        for index in 0..len {
            if !x[index].is_finite() || !y[index].is_finite() {
                continue;
            }
            series.points.push_back([x[index], y[index]]);
        }
        while series.points.len() > self.limit {
            series.points.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.series.clear();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        for series in &mut self.series {
            while series.points.len() > self.limit {
                series.points.pop_front();
            }
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use xserial_client::event::DecodedEntry;
    use xserial_core::protocol::DecodedData;
    use xserial_core::protocol::plot::{PlotFrame, SampleType};

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
        assert!(rendered.iter().all(|point| point[0] >= 0.0 && point[0] <= 99.0));
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
