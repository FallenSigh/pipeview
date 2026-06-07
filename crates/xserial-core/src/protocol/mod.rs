pub mod text;
pub mod hex;
pub mod plot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

/// Decoded result from a protocol decoder.
#[derive(Debug, Clone)]
pub enum DecodedData {
    Text(String),
    Hex(String),
    Plot(plot::PlotFrame),
    Binary(Vec<u8>),
}

impl DecodedData {
    pub fn summary(&self) -> String {
        match self {
            DecodedData::Text(s) => s.clone(),
            DecodedData::Hex(s) => s.clone(),
            DecodedData::Plot(frame) => {
                format!(
                    "Plot: {} channels × {} samples ({})",
                    frame.channels.len(),
                    frame.sample_count(),
                    frame.sample_type.name(),
                )
            }
            DecodedData::Binary(v) => format!("Binary: {} bytes", v.len()),
        }
    }
}

pub trait ProtocolDecoder: Send + Sync {
    fn name(&self) -> &str;
    fn decode(&self, frame: &[u8]) -> Option<DecodedData>;
}
