use serde::{Deserialize, Serialize};
use xserial_core::frame::{
    Endian, Framer,
    cobs::CobsFramer,
    fixed::FixedLengthFramer,
    length::{LengthConfig, LengthPrefixedFramer},
    line::{LineConfig, LineFramer},
};
use xserial_core::protocol::{
    ProtocolDecoder,
    hex::{HexConfig, HexDecoder},
    plot::{PlotConfig, PlotDecoder, PlotFormat, SampleType},
    text::{TextDecoder, TextEncoding},
};
use xserial_core::transport::TransportConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FramerConfig {
    Line {
        #[serde(default = "default_true")]
        strip_cr: bool,
        #[serde(default = "default_max_line")]
        max_line_len: usize,
    },
    Fixed {
        frame_len: usize,
    },
    Length {
        #[serde(default = "default_len_bytes")]
        len_bytes: usize,
        #[serde(default)]
        endian: Endian,
        #[serde(default)]
        length_includes_self: bool,
        #[serde(default = "default_max_payload")]
        max_payload: usize,
    },
    Cobs {
        #[serde(default = "default_max_frame")]
        max_frame: usize,
    },
}

fn default_true() -> bool {
    true
}
fn default_max_line() -> usize {
    1024 * 1024
}
fn default_len_bytes() -> usize {
    2
}
fn default_max_payload() -> usize {
    1024 * 1024
}
fn default_max_frame() -> usize {
    1024 * 1024
}

impl FramerConfig {
    pub fn build(self) -> Box<dyn Framer> {
        match self {
            FramerConfig::Line {
                strip_cr,
                max_line_len,
            } => Box::new(LineFramer::new(LineConfig {
                strip_cr,
                max_line_len,
            })),
            FramerConfig::Fixed { frame_len } => Box::new(FixedLengthFramer::new(frame_len)),
            FramerConfig::Length {
                len_bytes,
                endian,
                length_includes_self,
                max_payload,
            } => Box::new(LengthPrefixedFramer::new(LengthConfig {
                len_bytes,
                endian,
                length_includes_self,
                max_payload,
            })),
            FramerConfig::Cobs { max_frame } => Box::new(CobsFramer::new(max_frame)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecoderConfig {
    Text {
        #[serde(default)]
        encoding: TextEncoding,
    },
    Hex {
        #[serde(default)]
        uppercase: bool,
        #[serde(default = "default_sep")]
        separator: String,
        #[serde(default = "default_one")]
        bytes_per_group: usize,
        #[serde(default)]
        endian: Endian,
    },
    Plot {
        sample_type: SampleType,
        #[serde(default)]
        endian: Endian,
        #[serde(default = "default_one")]
        channels: usize,
        #[serde(default)]
        format: PlotFormat,
    },
}

fn default_sep() -> String {
    String::from(" ")
}
fn default_one() -> usize {
    1
}

impl DecoderConfig {
    pub fn build(self) -> Box<dyn ProtocolDecoder> {
        match self {
            DecoderConfig::Text { encoding } => Box::new(TextDecoder::new(encoding)),
            DecoderConfig::Hex {
                uppercase,
                separator,
                bytes_per_group,
                endian,
            } => Box::new(HexDecoder::new(HexConfig {
                uppercase,
                separator,
                bytes_per_group,
                endian,
            })),
            DecoderConfig::Plot {
                sample_type,
                endian,
                channels,
                format,
            } => Box::new(PlotDecoder::new(PlotConfig {
                sample_type,
                endian,
                channels,
                format,
            })),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub framer: FramerConfig,
    pub decoder: DecoderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub transport: TransportConfig,
    #[serde(default)]
    pub pipelines: Vec<PipelineConfig>,
    #[serde(default = "default_history")]
    pub history_limit: usize,
    #[serde(default)]
    pub auto_reconnect: bool,
}

fn default_history() -> usize {
    10_000
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            transport: TransportConfig::Tcp {
                addr: "127.0.0.1:8080".into(),
            },
            pipelines: Vec::new(),
            history_limit: default_history(),
            auto_reconnect: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xserial_core::protocol::Endian;
    use xserial_core::protocol::plot::{PlotFormat, SampleType};
    use xserial_core::protocol::text::TextEncoding;
    use xserial_core::transport::TransportConfig;

    #[test]
    fn framer_line_serde_roundtrip() {
        let json = r#"{"Line":{"strip_cr":true,"max_line_len":4096}}"#;
        let cfg: FramerConfig = serde_json::from_str(json).unwrap();
        match &cfg {
            FramerConfig::Line {
                strip_cr,
                max_line_len,
            } => {
                assert!(*strip_cr);
                assert_eq!(*max_line_len, 4096);
            }
            _ => panic!("expected Line"),
        }
        let back = serde_json::to_string(&cfg).unwrap();
        let _: FramerConfig = serde_json::from_str(&back).unwrap();
    }

    #[test]
    fn framer_fixed_serde() {
        let cfg: FramerConfig = serde_json::from_str(r#"{"Fixed":{"frame_len":128}}"#).unwrap();
        assert!(matches!(cfg, FramerConfig::Fixed { frame_len: 128 }));
    }

    #[test]
    fn framer_length_serde() {
        let json = r#"{"Length":{"len_bytes":4,"endian":"Little","length_includes_self":true}}"#;
        let cfg: FramerConfig = serde_json::from_str(json).unwrap();
        match cfg {
            FramerConfig::Length {
                len_bytes,
                endian,
                length_includes_self,
                ..
            } => {
                assert_eq!(len_bytes, 4);
                assert_eq!(endian, Endian::Little);
                assert!(length_includes_self);
            }
            _ => panic!("expected Length"),
        }
    }

    #[test]
    fn framer_cobs_serde() {
        let cfg: FramerConfig = serde_json::from_str(r#"{"Cobs":{"max_frame":8192}}"#).unwrap();
        assert!(matches!(cfg, FramerConfig::Cobs { max_frame: 8192 }));
    }

    #[test]
    fn decoder_text_serde() {
        let cfg: DecoderConfig = serde_json::from_str(r#"{"Text":{"encoding":"Latin1"}}"#).unwrap();
        match cfg {
            DecoderConfig::Text { encoding } => assert_eq!(encoding, TextEncoding::Latin1),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn decoder_hex_serde() {
        let json =
            r#"{"Hex":{"uppercase":true,"separator":":","bytes_per_group":2,"endian":"Little"}}"#;
        let cfg: DecoderConfig = serde_json::from_str(json).unwrap();
        match cfg {
            DecoderConfig::Hex {
                uppercase,
                separator,
                bytes_per_group,
                endian,
            } => {
                assert!(uppercase);
                assert_eq!(separator, ":");
                assert_eq!(bytes_per_group, 2);
                assert_eq!(endian, Endian::Little);
            }
            _ => panic!("expected Hex"),
        }
    }

    #[test]
    fn decoder_plot_serde() {
        let json = r#"{"Plot":{"sample_type":"F32","endian":"Little","channels":2,"format":"Interleaved"}}"#;
        let cfg: DecoderConfig = serde_json::from_str(json).unwrap();
        match cfg {
            DecoderConfig::Plot {
                sample_type,
                endian,
                channels,
                format,
            } => {
                assert_eq!(sample_type, SampleType::F32);
                assert_eq!(channels, 2);
                assert_eq!(format, PlotFormat::Interleaved);
                assert_eq!(endian, Endian::Little);
            }
            _ => panic!("expected Plot"),
        }
    }

    #[test]
    fn session_config_full_roundtrip() {
        let json = r#"{
            "transport":{"Tcp":{"addr":"192.168.1.1:9999"}},
            "pipelines":[
                {"name":"t","framer":{"Line":{}},"decoder":{"Text":{}}},
                {"name":"h","framer":{"Fixed":{"frame_len":16}},"decoder":{"Hex":{}}}
            ],
            "history_limit":5000,
            "auto_reconnect":true
        }"#;
        let cfg: SessionConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.auto_reconnect);
        assert_eq!(cfg.history_limit, 5000);
        assert_eq!(cfg.pipelines.len(), 2);
        assert!(
            matches!(&cfg.transport, TransportConfig::Tcp { addr } if addr == "192.168.1.1:9999")
        );
        let back = serde_json::to_string_pretty(&cfg).unwrap();
        let _: SessionConfig = serde_json::from_str(&back).unwrap();
    }

    #[test]
    fn framer_build_does_not_panic() {
        for cfg in [
            FramerConfig::Line {
                strip_cr: true,
                max_line_len: 256,
            },
            FramerConfig::Fixed { frame_len: 8 },
            FramerConfig::Length {
                len_bytes: 2,
                endian: Endian::Big,
                length_includes_self: false,
                max_payload: 65536,
            },
            FramerConfig::Cobs { max_frame: 1024 },
        ] {
            let f = cfg.build();
            assert_eq!(f.pending_len(), 0);
        }
    }

    #[test]
    fn decoder_build_returns_correct_name() {
        assert_eq!(
            DecoderConfig::Text {
                encoding: TextEncoding::Utf8
            }
            .build()
            .name(),
            "Text"
        );
        assert_eq!(
            DecoderConfig::Hex {
                uppercase: false,
                separator: " ".into(),
                bytes_per_group: 1,
                endian: Endian::Big
            }
            .build()
            .name(),
            "Hex"
        );
        assert_eq!(
            DecoderConfig::Plot {
                sample_type: SampleType::I16,
                endian: Endian::Big,
                channels: 2,
                format: PlotFormat::XY
            }
            .build()
            .name(),
            "Plot"
        );
    }
}
