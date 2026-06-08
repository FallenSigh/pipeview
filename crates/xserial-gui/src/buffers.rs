use std::time::{Duration, Instant};
use xserial_client::RingBuffer;
use xserial_client::event::DecodedEntry;
use xserial_core::protocol::DecodedData;

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
}
