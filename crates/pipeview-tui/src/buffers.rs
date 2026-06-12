use std::time::{Duration, Instant};

use pipeview_client::{DecodedEntry, RingBuffer};
use pipeview_core::protocol::DecodedData;

#[derive(Clone, Copy, PartialEq, Eq)]
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
    pub fn new(limit: usize) -> Self {
        Self {
            started_at: Instant::now(),
            lines: RingBuffer::new(limit),
        }
    }

    pub fn push(&mut self, entry: &DecodedEntry) {
        if let DecodedData::Text(text) = &entry.data {
            self.lines.push(ConsoleLine {
                elapsed: self.started_at.elapsed(),
                pipeline: entry.pipeline_name.clone(),
                text: text.clone(),
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

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.lines.set_limit(limit);
    }

    pub fn recent(&self, count: usize) -> Vec<ConsoleLine> {
        self.lines.drain_recent(count)
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
    pub fn new(limit: usize) -> Self {
        Self {
            started_at: Instant::now(),
            lines: RingBuffer::new(limit),
        }
    }

    pub fn push(&mut self, entry: &DecodedEntry) {
        if let DecodedData::Hex(hex) = &entry.data {
            self.lines.push(HexLine {
                elapsed: self.started_at.elapsed(),
                pipeline: entry.pipeline_name.clone(),
                ascii: decode_ascii(hex),
                hex: hex.clone(),
                direction: LineDirection::In,
            });
        }
    }

    pub fn push_outbound(&mut self, hex: String) {
        self.lines.push(HexLine {
            elapsed: self.started_at.elapsed(),
            pipeline: String::from("OUT"),
            ascii: decode_ascii(&hex),
            hex,
            direction: LineDirection::Out,
        });
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

    pub fn recent(&self, count: usize) -> Vec<HexLine> {
        self.lines.drain_recent(count)
    }
}

fn decode_ascii(hex: &str) -> String {
    hex::decode(hex.replace(' ', ""))
        .map(|bytes| {
            bytes
                .into_iter()
                .map(|byte| {
                    if byte.is_ascii_graphic() || byte == b' ' {
                        byte as char
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .unwrap_or_else(|_| String::from("[invalid hex]"))
}
