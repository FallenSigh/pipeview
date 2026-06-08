use super::{DecodedData, ProtocolDecoder};
use crate::protocol::Endian;
use serde::{Deserialize, Serialize};

/// Numeric type of samples in a plot frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleType {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl SampleType {
    pub fn name(&self) -> &str {
        match self {
            SampleType::I8 => "i8",
            SampleType::U8 => "u8",
            SampleType::I16 => "i16",
            SampleType::U16 => "u16",
            SampleType::I32 => "i32",
            SampleType::U32 => "u32",
            SampleType::I64 => "i64",
            SampleType::U64 => "u64",
            SampleType::F32 => "f32",
            SampleType::F64 => "f64",
        }
    }

    pub fn byte_size(&self) -> usize {
        match self {
            SampleType::I8 | SampleType::U8 => 1,
            SampleType::I16 | SampleType::U16 => 2,
            SampleType::I32 | SampleType::U32 | SampleType::F32 => 4,
            SampleType::I64 | SampleType::U64 | SampleType::F64 => 8,
        }
    }
}

/// Channel layout for multi-channel plot data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlotFormat {
    #[default]
    Interleaved,
    Block,
    XY,
}

/// Configuration for the plot protocol decoder.
#[derive(Debug, Clone)]
pub struct PlotConfig {
    pub sample_type: SampleType,
    pub endian: Endian,
    pub channels: usize,
    pub format: PlotFormat,
}

impl Default for PlotConfig {
    fn default() -> Self {
        Self {
            sample_type: SampleType::F32,
            endian: Endian::Little,
            channels: 1,
            format: PlotFormat::Interleaved,
        }
    }
}

/// A decoded plot frame containing numeric channel data.
#[derive(Debug, Clone)]
pub struct PlotFrame {
    pub channels: Vec<Vec<f64>>,
    pub raw: Vec<u8>,
    pub sample_type: SampleType,
    pub format: PlotFormat,
}

impl PlotFrame {
    pub fn sample_count(&self) -> usize {
        self.channels.first().map_or(0, |c| c.len())
    }
}

/// Plot protocol decoder.
///
/// Interprets binary frames as numeric samples and organizes them into
/// channels according to the configured layout.
#[derive(Debug, Clone)]
pub struct PlotDecoder {
    config: PlotConfig,
}

impl PlotDecoder {
    pub fn new(config: PlotConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &PlotConfig {
        &self.config
    }

    fn read_one_sample(&self, data: &[u8]) -> Option<f64> {
        let size = self.config.sample_type.byte_size();
        if data.len() < size {
            return None;
        }
        let bytes = &data[..size];
        let raw = match self.config.sample_type {
            SampleType::I8 => bytes[0] as i8 as f64,
            SampleType::U8 => bytes[0] as f64,
            SampleType::I16 => {
                let b = [bytes[0], bytes[1]];
                let val = match self.config.endian {
                    Endian::Big => i16::from_be_bytes(b),
                    Endian::Little => i16::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::U16 => {
                let b = [bytes[0], bytes[1]];
                let val = match self.config.endian {
                    Endian::Big => u16::from_be_bytes(b),
                    Endian::Little => u16::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::I32 => {
                let b = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let val = match self.config.endian {
                    Endian::Big => i32::from_be_bytes(b),
                    Endian::Little => i32::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::U32 => {
                let b = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let val = match self.config.endian {
                    Endian::Big => u32::from_be_bytes(b),
                    Endian::Little => u32::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::I64 => {
                let b = [
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ];
                let val = match self.config.endian {
                    Endian::Big => i64::from_be_bytes(b),
                    Endian::Little => i64::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::U64 => {
                let b = [
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ];
                let val = match self.config.endian {
                    Endian::Big => u64::from_be_bytes(b),
                    Endian::Little => u64::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::F32 => {
                let b = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let val = match self.config.endian {
                    Endian::Big => f32::from_be_bytes(b),
                    Endian::Little => f32::from_le_bytes(b),
                };
                val as f64
            }
            SampleType::F64 => {
                let b = [
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ];
                match self.config.endian {
                    Endian::Big => f64::from_be_bytes(b),
                    Endian::Little => f64::from_le_bytes(b),
                }
            }
        };
        Some(raw)
    }
}

impl ProtocolDecoder for PlotDecoder {
    fn name(&self) -> &str {
        "Plot"
    }

    fn decode(&self, frame: &[u8]) -> Option<DecodedData> {
        let sample_size = self.config.sample_type.byte_size();
        let num_channels = match self.config.format {
            PlotFormat::XY => 2,
            _ => self.config.channels.max(1),
        };

        let total_samples = frame.len() / sample_size;
        if total_samples == 0 {
            return None;
        }

        let samples_per_channel = total_samples / num_channels;
        if samples_per_channel == 0 {
            return None;
        }

        // Read all samples
        let mut flat: Vec<f64> = Vec::with_capacity(total_samples);
        let mut offset = 0;
        while offset + sample_size <= frame.len() {
            let val = self.read_one_sample(&frame[offset..])?;
            flat.push(val);
            offset += sample_size;
        }

        // Distribute into channels
        let mut channels: Vec<Vec<f64>> =
            vec![Vec::with_capacity(samples_per_channel); num_channels];
        match self.config.format {
            PlotFormat::Interleaved | PlotFormat::XY => {
                for (i, val) in flat.into_iter().enumerate() {
                    let ch = i % num_channels;
                    if channels[ch].len() < samples_per_channel {
                        channels[ch].push(val);
                    }
                }
            }
            PlotFormat::Block => {
                for (ch, channel) in channels.iter_mut().enumerate() {
                    let start = ch * samples_per_channel;
                    let end = start + samples_per_channel;
                    if end <= flat.len() {
                        channel.extend_from_slice(&flat[start..end]);
                    }
                }
            }
        }

        Some(DecodedData::Plot(PlotFrame {
            channels,
            raw: frame.to_vec(),
            sample_type: self.config.sample_type,
            format: self.config.format,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_type_names() {
        assert_eq!(SampleType::I8.name(), "i8");
        assert_eq!(SampleType::U16.name(), "u16");
        assert_eq!(SampleType::F32.name(), "f32");
        assert_eq!(SampleType::F64.name(), "f64");
    }

    #[test]
    fn sample_type_sizes() {
        assert_eq!(SampleType::I8.byte_size(), 1);
        assert_eq!(SampleType::U8.byte_size(), 1);
        assert_eq!(SampleType::I16.byte_size(), 2);
        assert_eq!(SampleType::U16.byte_size(), 2);
        assert_eq!(SampleType::I32.byte_size(), 4);
        assert_eq!(SampleType::F32.byte_size(), 4);
        assert_eq!(SampleType::I64.byte_size(), 8);
        assert_eq!(SampleType::F64.byte_size(), 8);
    }

    #[test]
    fn plot_single_channel_u8() {
        let cfg = PlotConfig {
            sample_type: SampleType::U8,
            channels: 1,
            ..PlotConfig::default()
        };
        let d = PlotDecoder::new(cfg);
        let data = vec![10u8, 20, 30, 40];
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 1);
                assert_eq!(frame.channels[0], vec![10.0, 20.0, 30.0, 40.0]);
                assert_eq!(frame.sample_count(), 4);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_single_channel_i16_le() {
        let cfg = PlotConfig {
            sample_type: SampleType::I16,
            endian: Endian::Little,
            channels: 1,
            ..PlotConfig::default()
        };
        let d = PlotDecoder::new(cfg);
        let data = vec![0x00, 0x80, 0xff, 0x7f]; // -32768, 32767 in LE
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 1);
                assert_eq!(frame.channels[0], vec![-32768.0, 32767.0]);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_single_channel_f32_le() {
        let cfg = PlotConfig {
            sample_type: SampleType::F32,
            endian: Endian::Little,
            channels: 1,
            ..PlotConfig::default()
        };
        let d = PlotDecoder::new(cfg);
        // 1.0, -2.5 in f32 LE
        let mut data = Vec::new();
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&(-2.5f32).to_le_bytes());
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 1);
                assert_eq!(frame.channels[0].len(), 2);
                assert!((frame.channels[0][0] - 1.0).abs() < 1e-6);
                assert!((frame.channels[0][1] - (-2.5)).abs() < 1e-6);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_two_channels_interleaved_u16_le() {
        let cfg = PlotConfig {
            sample_type: SampleType::U16,
            endian: Endian::Little,
            channels: 2,
            format: PlotFormat::Interleaved,
        };
        let d = PlotDecoder::new(cfg);
        // ch0: 100, 300  ch1: 200, 400
        let data = vec![
            100u16.to_le_bytes(),
            200u16.to_le_bytes(),
            300u16.to_le_bytes(),
            400u16.to_le_bytes(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<u8>>();
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 2);
                assert_eq!(frame.channels[0], vec![100.0, 300.0]);
                assert_eq!(frame.channels[1], vec![200.0, 400.0]);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_two_channels_block_u16_be() {
        let cfg = PlotConfig {
            sample_type: SampleType::U16,
            endian: Endian::Big,
            channels: 2,
            format: PlotFormat::Block,
        };
        let d = PlotDecoder::new(cfg);
        // ch0: 10, 20  ch1: 30, 40
        let data = vec![
            10u16.to_be_bytes(),
            20u16.to_be_bytes(),
            30u16.to_be_bytes(),
            40u16.to_be_bytes(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<u8>>();
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 2);
                assert_eq!(frame.channels[0], vec![10.0, 20.0]);
                assert_eq!(frame.channels[1], vec![30.0, 40.0]);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_xy_format() {
        let cfg = PlotConfig {
            sample_type: SampleType::U16,
            endian: Endian::Little,
            channels: 0, // ignored for XY
            format: PlotFormat::XY,
        };
        let d = PlotDecoder::new(cfg);
        // (x,y) pairs: (10, 100), (20, 200)
        let data = vec![
            10u16.to_le_bytes(),
            100u16.to_le_bytes(),
            20u16.to_le_bytes(),
            200u16.to_le_bytes(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<u8>>();
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 2);
                assert_eq!(frame.channels[0], vec![10.0, 20.0]); // X
                assert_eq!(frame.channels[1], vec![100.0, 200.0]); // Y
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_empty_frame_returns_none() {
        let d = PlotDecoder::new(PlotConfig::default());
        assert!(d.decode(&[]).is_none());
    }

    #[test]
    fn plot_too_few_bytes_returns_none() {
        let cfg = PlotConfig {
            sample_type: SampleType::F64,
            channels: 1,
            ..PlotConfig::default()
        };
        let d = PlotDecoder::new(cfg);
        assert!(d.decode(&[0x00, 0x01, 0x02]).is_none());
    }

    #[test]
    fn plot_insufficient_for_channels_returns_none() {
        let cfg = PlotConfig {
            sample_type: SampleType::U8,
            endian: Endian::Little,
            channels: 3,
            format: PlotFormat::Interleaved,
        };
        let d = PlotDecoder::new(cfg);
        // 2 bytes for 3 channels → not enough for 1 full sample per channel
        assert!(d.decode(&[1, 2]).is_none());
    }

    #[test]
    fn plot_trailing_partial_sample_ignored() {
        let cfg = PlotConfig {
            sample_type: SampleType::U32,
            channels: 1,
            ..PlotConfig::default()
        };
        let d = PlotDecoder::new(cfg);
        // 7 bytes: 1 full u32 (4 bytes) + 3 trailing bytes
        let data = vec![1u32.to_le_bytes().to_vec(), vec![0xff; 3]].concat();
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels.len(), 1);
                assert_eq!(frame.channels[0], vec![1.0]);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_name() {
        let d = PlotDecoder::new(PlotConfig::default());
        assert_eq!(d.name(), "Plot");
    }

    #[test]
    fn plot_frame_sample_count_empty() {
        let frame = PlotFrame {
            channels: vec![],
            raw: vec![],
            sample_type: SampleType::U8,
            format: PlotFormat::Interleaved,
        };
        assert_eq!(frame.sample_count(), 0);
    }

    #[test]
    fn plot_raw_preserved() {
        let cfg = PlotConfig {
            sample_type: SampleType::U8,
            channels: 1,
            ..PlotConfig::default()
        };
        let d = PlotDecoder::new(cfg);
        let data = vec![1, 2, 3];
        let result = d.decode(&data).unwrap();
        match result {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.raw, data);
                assert_eq!(frame.format, PlotFormat::Interleaved);
            }
            other => panic!("expected Plot, got {:?}", other),
        }
    }

    #[test]
    fn plot_decoder_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PlotDecoder>();
    }
}
