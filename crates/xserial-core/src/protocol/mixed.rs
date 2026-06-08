use super::plot::{PlotConfig, PlotDecoder, PlotFormat, SampleType};
use super::text::{TextDecoder, TextEncoding};
use super::{DecodedData, Endian, ProtocolDecoder};

pub const MIXED_PLOT_ESCAPE: u8 = 0x1E;
pub const MIXED_PLOT_MARKER: u8 = b'P';
pub const MIXED_FRAME_KIND_TEXT: u8 = 0x00;
pub const MIXED_FRAME_KIND_PLOT: u8 = 0x01;

const PLOT_PACKET_MAGIC: &[u8; 2] = b"XP";
const PLOT_PACKET_VERSION: u8 = 1;
const PLOT_PACKET_HEADER_LEN: usize = 13;

#[derive(Debug, Clone, Copy)]
pub struct MixedTextPlotConfig {
    pub encoding: TextEncoding,
}

impl Default for MixedTextPlotConfig {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::Utf8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MixedTextPlotDecoder {
    config: MixedTextPlotConfig,
}

impl MixedTextPlotDecoder {
    pub fn new(config: MixedTextPlotConfig) -> Self {
        Self { config }
    }

    pub fn build_plot_packet(
        sample_type: SampleType,
        endian: Endian,
        channels: usize,
        format: PlotFormat,
        samples_per_channel: usize,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(PLOT_PACKET_HEADER_LEN + payload.len());
        packet.extend_from_slice(PLOT_PACKET_MAGIC);
        packet.push(PLOT_PACKET_VERSION);
        packet.push(plot_format_to_u8(format));
        packet.push(sample_type_to_u8(sample_type));
        packet.push(match endian {
            Endian::Little => 0,
            Endian::Big => 1,
        });
        packet.push(channels.min(u8::MAX as usize) as u8);
        packet.extend_from_slice(&(samples_per_channel.min(u16::MAX as usize) as u16).to_le_bytes());
        packet.extend_from_slice(&(payload.len().min(u32::MAX as usize) as u32).to_le_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    fn decode_plot(&self, frame: &[u8]) -> Option<DecodedData> {
        if frame.len() < PLOT_PACKET_HEADER_LEN {
            return None;
        }
        if &frame[..2] != PLOT_PACKET_MAGIC {
            return None;
        }
        if frame[2] != PLOT_PACKET_VERSION {
            return None;
        }

        let format = plot_format_from_u8(frame[3])?;
        let sample_type = sample_type_from_u8(frame[4])?;
        let endian = match frame[5] {
            0 => Endian::Little,
            1 => Endian::Big,
            _ => return None,
        };
        let channels = frame[6] as usize;
        let samples_per_channel = u16::from_le_bytes([frame[7], frame[8]]) as usize;
        let payload_len = u32::from_le_bytes([frame[9], frame[10], frame[11], frame[12]]) as usize;
        let payload = frame.get(PLOT_PACKET_HEADER_LEN..PLOT_PACKET_HEADER_LEN + payload_len)?;

        let num_channels = match format {
            PlotFormat::XY => 2,
            _ => channels.max(1),
        };
        if matches!(format, PlotFormat::XY) && channels != 2 {
            return None;
        }

        let expected_len = sample_type
            .byte_size()
            .checked_mul(num_channels)?
            .checked_mul(samples_per_channel)?;
        if payload.len() != expected_len {
            return None;
        }

        let decoder = PlotDecoder::new(PlotConfig {
            sample_type,
            endian,
            channels: num_channels,
            format,
        });
        decoder.decode(payload)
    }
}

impl ProtocolDecoder for MixedTextPlotDecoder {
    fn name(&self) -> &str {
        "MixedTextPlot"
    }

    fn decode(&self, frame: &[u8]) -> Option<DecodedData> {
        let (&kind, payload) = frame.split_first()?;
        match kind {
            MIXED_FRAME_KIND_TEXT => TextDecoder::new(self.config.encoding).decode(payload),
            MIXED_FRAME_KIND_PLOT => self.decode_plot(payload),
            _ => None,
        }
    }
}

fn plot_format_to_u8(format: PlotFormat) -> u8 {
    match format {
        PlotFormat::Interleaved => 0,
        PlotFormat::Block => 1,
        PlotFormat::XY => 2,
    }
}

fn plot_format_from_u8(value: u8) -> Option<PlotFormat> {
    match value {
        0 => Some(PlotFormat::Interleaved),
        1 => Some(PlotFormat::Block),
        2 => Some(PlotFormat::XY),
        _ => None,
    }
}

fn sample_type_to_u8(sample_type: SampleType) -> u8 {
    match sample_type {
        SampleType::I8 => 0,
        SampleType::U8 => 1,
        SampleType::I16 => 2,
        SampleType::U16 => 3,
        SampleType::I32 => 4,
        SampleType::U32 => 5,
        SampleType::I64 => 6,
        SampleType::U64 => 7,
        SampleType::F32 => 8,
        SampleType::F64 => 9,
    }
}

fn sample_type_from_u8(value: u8) -> Option<SampleType> {
    match value {
        0 => Some(SampleType::I8),
        1 => Some(SampleType::U8),
        2 => Some(SampleType::I16),
        3 => Some(SampleType::U16),
        4 => Some(SampleType::I32),
        5 => Some(SampleType::U32),
        6 => Some(SampleType::I64),
        7 => Some(SampleType::U64),
        8 => Some(SampleType::F32),
        9 => Some(SampleType::F64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_decoder_decodes_text_frame() {
        let decoder = MixedTextPlotDecoder::new(MixedTextPlotConfig::default());
        let frame = b"\0hello";
        assert!(matches!(decoder.decode(frame), Some(DecodedData::Text(ref s)) if s == "hello"));
    }

    #[test]
    fn mixed_decoder_decodes_plot_frame() {
        let decoder = MixedTextPlotDecoder::new(MixedTextPlotConfig::default());
        let payload = [0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40];
        let packet = MixedTextPlotDecoder::build_plot_packet(
            SampleType::F32,
            Endian::Little,
            1,
            PlotFormat::Interleaved,
            2,
            &payload,
        );
        let frame = [vec![MIXED_FRAME_KIND_PLOT], packet].concat();
        let decoded = decoder.decode(&frame).unwrap();
        match decoded {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 1);
                assert_eq!(frame.channels[0], vec![1.0, 2.0]);
            }
            other => panic!("expected Plot, got {other:?}"),
        }
    }

    #[test]
    fn mixed_decoder_rejects_bad_payload_length() {
        let decoder = MixedTextPlotDecoder::new(MixedTextPlotConfig::default());
        let packet = MixedTextPlotDecoder::build_plot_packet(
            SampleType::F32,
            Endian::Little,
            1,
            PlotFormat::Interleaved,
            2,
            &[0x00, 0x00, 0x80, 0x3f],
        );
        let frame = [vec![MIXED_FRAME_KIND_PLOT], packet].concat();
        assert!(decoder.decode(&frame).is_none());
    }
}
