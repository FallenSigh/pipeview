use super::{DecodedData, ProtocolDecoder};
use crate::protocol::Endian;

#[derive(Debug, Clone)]
pub struct HexConfig {
    pub uppercase: bool,
    pub separator: String,
    pub bytes_per_group: usize,
    pub endian: Endian,
}

impl Default for HexConfig {
    fn default() -> Self {
        Self {
            uppercase: false,
            separator: String::from(" "),
            bytes_per_group: 1,
            endian: Endian::Big,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HexDecoder {
    config: HexConfig,
}

impl HexDecoder {
    pub fn new(config: HexConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &HexConfig {
        &self.config
    }
}

impl ProtocolDecoder for HexDecoder {
    fn name(&self) -> &str {
        "Hex"
    }

    fn decode(&self, frame: &[u8]) -> Option<DecodedData> {
        if frame.is_empty() {
            return Some(DecodedData::Hex(String::new()));
        }

        let group_size = self.config.bytes_per_group.max(1);
        let mut result = String::new();
        let chunks: Vec<&[u8]> = frame.chunks(group_size).collect();
        let total = chunks.len();

        for (i, chunk) in chunks.iter().enumerate() {
            if i > 0 {
                result.push_str(&self.config.separator);
            }

            let is_last = i == total - 1;
            let mut display: Vec<u8> = chunk.to_vec();

            if !is_last || chunk.len() == group_size {
                // Complete group — apply endian reversal
                if self.config.endian == Endian::Little {
                    display.reverse();
                }
            }
            // Incomplete trailing group — display bytes in original order

            for b in &display {
                let hex_byte = if self.config.uppercase {
                    format!("{:02X}", b)
                } else {
                    format!("{:02x}", b)
                };
                result.push_str(&hex_byte);
            }
        }

        Some(DecodedData::Hex(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_default_single_byte_lower() {
        let d = HexDecoder::new(HexConfig::default());
        let result = d.decode(&[0x0a, 0x1b, 0xff]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "0a 1b ff"));
    }

    #[test]
    fn hex_uppercase() {
        let cfg = HexConfig {
            uppercase: true,
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0x0a, 0x1b, 0xff]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "0A 1B FF"));
    }

    #[test]
    fn hex_no_separator() {
        let cfg = HexConfig {
            separator: String::new(),
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0xde, 0xad, 0xbe, 0xef]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "deadbeef"));
    }

    #[test]
    fn hex_custom_separator() {
        let cfg = HexConfig {
            separator: String::from(":"),
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0x01, 0x02, 0x03]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "01:02:03"));
    }

    #[test]
    fn hex_group_u16_big_endian() {
        let cfg = HexConfig {
            bytes_per_group: 2,
            endian: Endian::Big,
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0x12, 0x34, 0x56, 0x78]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "1234 5678"));
    }

    #[test]
    fn hex_group_u16_little_endian() {
        let cfg = HexConfig {
            bytes_per_group: 2,
            endian: Endian::Little,
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0x12, 0x34, 0x56, 0x78]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "3412 7856"));
    }

    #[test]
    fn hex_group_u32_little_endian() {
        let cfg = HexConfig {
            bytes_per_group: 4,
            endian: Endian::Little,
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "04030201 08070605"));
    }

    #[test]
    fn hex_incomplete_trailing_group_no_reversal() {
        let cfg = HexConfig {
            bytes_per_group: 2,
            endian: Endian::Little,
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        // 5 bytes: two complete groups + 1 trailing byte
        let result = d.decode(&[0x12, 0x34, 0x56, 0x78, 0x9a]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "3412 7856 9a"));
    }

    #[test]
    fn hex_empty_input() {
        let d = HexDecoder::new(HexConfig::default());
        let result = d.decode(&[]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s.is_empty()));
    }

    #[test]
    fn hex_single_byte() {
        let d = HexDecoder::new(HexConfig::default());
        let result = d.decode(&[0x42]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "42"));
    }

    #[test]
    fn hex_zero_bytes() {
        let d = HexDecoder::new(HexConfig::default());
        let result = d.decode(&[0x00, 0x00, 0x00]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "00 00 00"));
    }

    #[test]
    fn hex_group_size_one_never_reverses() {
        let cfg = HexConfig {
            bytes_per_group: 1,
            endian: Endian::Little,
            ..HexConfig::default()
        };
        let d = HexDecoder::new(cfg);
        let result = d.decode(&[0x01, 0x02, 0x03]);
        assert!(matches!(result, Some(DecodedData::Hex(ref s)) if s == "01 02 03"));
    }

    #[test]
    fn hex_name() {
        let d = HexDecoder::new(HexConfig::default());
        assert_eq!(d.name(), "Hex");
    }

    #[test]
    fn hex_config_accessor() {
        let cfg = HexConfig {
            bytes_per_group: 4,
            endian: Endian::Little,
            uppercase: true,
            separator: String::from("-"),
        };
        let d = HexDecoder::new(cfg.clone());
        assert_eq!(d.config().bytes_per_group, 4);
        assert_eq!(d.config().endian, Endian::Little);
        assert!(d.config().uppercase);
        assert_eq!(d.config().separator, "-");
    }
}
