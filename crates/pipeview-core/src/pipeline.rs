use crate::frame::Framer;
use crate::protocol::{DecodedData, ProtocolDecoder};
use tracing::debug;

/// A self-contained framing + decoding pipeline.
///
/// Each pipeline has its own `Framer` (with independent internal buffer)
/// and its own `ProtocolDecoder`.  Feed the same byte chunk to multiple
/// pipelines and each will extract and decode only the frames it understands.
pub struct Pipeline {
    name: String,
    framer: Box<dyn Framer>,
    decoder: Box<dyn ProtocolDecoder>,
}

impl Pipeline {
    pub fn new(
        name: impl Into<String>,
        framer: Box<dyn Framer>,
        decoder: Box<dyn ProtocolDecoder>,
    ) -> Self {
        Self {
            name: name.into(),
            framer,
            decoder,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn feed_inner(&mut self, data: &[u8]) -> Vec<DecodedData> {
        self.framer
            .feed(data)
            .into_iter()
            .filter_map(|frame| self.decoder.decode(&frame))
            .collect()
    }

    fn flush_inner(&mut self) -> Vec<DecodedData> {
        self.framer
            .flush()
            .into_iter()
            .filter_map(|frame| self.decoder.decode(&frame))
            .collect()
    }

    fn reset_inner(&mut self) {
        self.framer.reset();
    }
}

/// One decoded result from a pipeline, tagged with the pipeline name.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub pipeline_name: String,
    pub data: DecodedData,
}

/// Manages multiple [`Pipeline`]s, feeding the same byte stream to all of them.
///
/// ```
/// use pipeview_core::pipeline::{MultiPipeline, Pipeline};
/// use pipeview_core::frame::line::{LineFramer, LineConfig};
/// use pipeview_core::frame::fixed::FixedLengthFramer;
/// use pipeview_core::protocol::text::{TextDecoder, TextEncoding};
/// use pipeview_core::protocol::hex::{HexDecoder, HexConfig};
///
/// let mut mp = MultiPipeline::new();
/// mp.add(
///     Pipeline::new("text",
///         Box::new(LineFramer::new(LineConfig::default())),
///         Box::new(TextDecoder::new(TextEncoding::Utf8)),
///     )
/// );
/// mp.add(
///     Pipeline::new("hex",
///         Box::new(FixedLengthFramer::new(8)),
///         Box::new(HexDecoder::new(HexConfig::default())),
///     )
/// );
///
/// let results = mp.feed(b"hello\n");
/// // "text" pipeline produced one line, "hex" pipeline is still buffering
/// ```
pub struct MultiPipeline {
    pipelines: Vec<Pipeline>,
}

impl MultiPipeline {
    pub fn new() -> Self {
        Self {
            pipelines: Vec::new(),
        }
    }

    pub fn add(&mut self, pipeline: Pipeline) {
        self.pipelines.push(pipeline);
    }

    pub fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }

    /// Feed a chunk of bytes to every pipeline.
    ///
    /// Returns all decoded results from all pipelines, in the order the
    /// pipelines were added.  Pipelines that produced no complete frames
    /// (or whose decoder returned `None`) are simply absent from the output.
    pub fn feed(&mut self, data: &[u8]) -> Vec<PipelineResult> {
        let mut results = Vec::new();
        for p in &mut self.pipelines {
            let name = p.name.clone();
            for decoded in p.feed_inner(data) {
                results.push(PipelineResult {
                    pipeline_name: name.clone(),
                    data: decoded,
                });
            }
        }
        if !results.is_empty() {
            debug!(
                count = results.len(),
                bytes = data.len(),
                "pipeline produced results"
            );
        }
        results
    }

    /// Flush every pipeline.
    ///
    /// Call after the transport disconnects to drain any incomplete
    /// frames still buffered inside the framers.
    pub fn flush(&mut self) -> Vec<PipelineResult> {
        let mut results = Vec::new();
        for p in &mut self.pipelines {
            let name = p.name.clone();
            for decoded in p.flush_inner() {
                results.push(PipelineResult {
                    pipeline_name: name.clone(),
                    data: decoded,
                });
            }
        }
        results
    }

    /// Reset every pipeline to a clean state (discards all buffered data).
    pub fn reset(&mut self) {
        for p in &mut self.pipelines {
            p.reset_inner();
        }
    }

    /// Return `(name, pending_bytes)` for each pipeline that has buffered data.
    pub fn pending_bytes(&self) -> Vec<(&str, usize)> {
        self.pipelines
            .iter()
            .map(|p| (p.name.as_str(), p.framer.pending_len()))
            .filter(|(_, len)| *len > 0)
            .collect()
    }
}

impl Default for MultiPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::fixed::FixedLengthFramer;
    use crate::frame::length::{LengthConfig, LengthPrefixedFramer};
    use crate::frame::line::{LineConfig, LineFramer};
    use crate::protocol::Endian;
    use crate::protocol::hex::{HexConfig, HexDecoder};
    use crate::protocol::plot::{PlotConfig, PlotDecoder, PlotFormat, SampleType};
    use crate::protocol::text::{TextDecoder, TextEncoding};

    #[test]
    fn multi_text_and_hex_same_stream() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "hex",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        // Both pipelines use LineFramer → both see "hello\n" and "42\n"
        let results = mp.feed(b"hello\n42\n");

        assert_eq!(results.len(), 4);
        assert_eq!(results[0].pipeline_name, "text");
        assert!(matches!(&results[0].data, DecodedData::Text(s) if s == "hello"));
        assert_eq!(results[1].pipeline_name, "text");
        assert!(matches!(&results[1].data, DecodedData::Text(s) if s == "42"));
        assert_eq!(results[2].pipeline_name, "hex");
        assert!(matches!(&results[2].data, DecodedData::Hex(s) if s == "68 65 6c 6c 6f"));
        assert_eq!(results[3].pipeline_name, "hex");
        assert!(matches!(&results[3].data, DecodedData::Hex(s) if s == "34 32"));
    }

    #[test]
    fn multi_text_success_hex_silent_on_binary_trash() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "hex",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        let results = mp.feed(b"valid\n\xff\xfe\xfd\n");

        // Text pipeline: "valid" ok, binary line fails UTF-8 → only 1 result
        // Hex pipeline: both lines decode successfully → 2 results
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].pipeline_name, "text");
        assert!(matches!(&results[0].data, DecodedData::Text(s) if s == "valid"));
        assert_eq!(results[1].pipeline_name, "hex");
        assert!(matches!(&results[1].data, DecodedData::Hex(_)));
        assert_eq!(results[2].pipeline_name, "hex");
        assert!(matches!(&results[2].data, DecodedData::Hex(_)));
    }

    #[test]
    fn multi_different_framers_per_pipeline() {
        let mut mp = MultiPipeline::new();
        // Text uses LineFramer, hex uses FixedLengthFramer(4)
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "hex",
            Box::new(FixedLengthFramer::new(4)),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        let results = mp.feed(b"abc\n1234");

        // text: sees "abc\n" → produces "abc"
        // hex: sees 8 bytes → produces 2 fixed frames: "abc\n" and "1234"
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].pipeline_name, "text");
        assert!(matches!(&results[0].data, DecodedData::Text(s) if s == "abc"));

        let hex_results: Vec<_> = results
            .iter()
            .filter(|r| r.pipeline_name == "hex")
            .collect();
        assert_eq!(hex_results.len(), 2);
    }

    #[test]
    fn multi_flush_drains_all() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "hex",
            Box::new(FixedLengthFramer::new(4)),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        // "partial" = 7 bytes. Line: no \n → buffers. Fixed(4): 1 frame "part", 3 left.
        mp.feed(b"partial");

        let flushed = mp.flush();
        // Line flush → "partial" as text
        // Fixed flush → "ial" as hex ("69 61 6c")
        assert_eq!(flushed.len(), 2);
        assert_eq!(flushed[0].pipeline_name, "text");
        assert!(matches!(&flushed[0].data, DecodedData::Text(s) if s == "partial"));
        assert_eq!(flushed[1].pipeline_name, "hex");
        assert!(matches!(&flushed[1].data, DecodedData::Hex(s) if s == "69 61 6c"));
    }

    #[test]
    fn multi_reset_clears_all_state() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "hex",
            Box::new(FixedLengthFramer::new(4)),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        mp.feed(b"unused data here that never completes");
        assert_eq!(mp.pending_bytes().len(), 2);

        mp.reset();
        assert!(mp.pending_bytes().is_empty());

        // After reset, pipelines work normally
        let results = mp.feed(b"ok\nabcd");
        assert!(!results.is_empty());
    }

    #[test]
    fn multi_pending_bytes_reporting() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "t",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "h",
            Box::new(FixedLengthFramer::new(100)),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        mp.feed(b"hello world");
        let pending: Vec<_> = mp.pending_bytes();
        assert_eq!(pending.len(), 2);
        // Both have the same 11 bytes buffered
        assert_eq!(pending[0], ("t", 11));
        assert_eq!(pending[1], ("h", 11));
    }

    #[test]
    fn multi_empty_feed_no_results() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));

        let results = mp.feed(b"");
        assert!(results.is_empty());
    }

    #[test]
    fn multi_no_pipelines() {
        let mut mp = MultiPipeline::new();
        let results = mp.feed(b"data");
        assert!(results.is_empty());
        assert_eq!(mp.pipeline_count(), 0);
    }

    #[test]
    fn multi_pipeline_count() {
        let mut mp = MultiPipeline::new();
        assert_eq!(mp.pipeline_count(), 0);
        mp.add(Pipeline::new(
            "a",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        assert_eq!(mp.pipeline_count(), 1);
    }

    #[test]
    fn multi_text_and_plot_mixed_stream() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "text",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        mp.add(Pipeline::new(
            "plot",
            Box::new(FixedLengthFramer::new(8)), // 2 × f32 LE per frame
            Box::new(PlotDecoder::new(PlotConfig {
                sample_type: SampleType::F32,
                endian: Endian::Little,
                channels: 1,
                format: PlotFormat::Interleaved,
            })),
        ));

        // 10 bytes of text + 8 bytes of f32 samples = 18 total.
        // FixedLengthFramer(8) extracts 2 frames. The first frame is ASCII
        // bytes which happen to decode as valid (but meaningless) f32 values
        // — PlotDecoder can't reject them. This is by design: garbage-in,
        // garbage-out; the UI layer decides what to display.
        let mut data = b"status ok\n".to_vec();
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        let results = mp.feed(&data);

        // text: 1 line, plot: 2 fixed frames (first = garbage f32, second = real)
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].pipeline_name, "text");
        assert!(matches!(&results[0].data, DecodedData::Text(s) if s == "status ok"));
    }

    #[test]
    fn multi_length_prefixed_and_line_stacked() {
        let mut mp = MultiPipeline::new();
        mp.add(Pipeline::new(
            "line",
            Box::new(LineFramer::new(LineConfig::default())),
            Box::new(TextDecoder::new(TextEncoding::Utf8)),
        ));
        // Second pipeline uses a different framer on the same bytes.
        // The length framer sees "he" (from "hello\n") as a bogus length header
        // and silently produces no frames — that's expected.
        mp.add(Pipeline::new(
            "len",
            Box::new(LengthPrefixedFramer::new(LengthConfig::default())),
            Box::new(HexDecoder::new(HexConfig::default())),
        ));

        let mut data = b"hello\n".to_vec();
        data.extend_from_slice(&3u16.to_be_bytes());
        data.extend_from_slice(b"abc");

        let results = mp.feed(&data);
        // Only the line pipeline produces output; length pipeline silently
        // ignores bytes that don't form valid length-prefixed frames.
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0].data, DecodedData::Text(s) if s == "hello"));
    }
}
