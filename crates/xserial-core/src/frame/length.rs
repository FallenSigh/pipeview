use super::{Endian, Framer};

/// Configuration for the length-prefixed framer.
#[derive(Debug, Clone)]
pub struct LengthConfig {
    /// Size of the length field in bytes: 1, 2, or 4.
    pub len_bytes: usize,
    pub endian: Endian,
    /// When true, the length value includes the length field itself.
    pub length_includes_self: bool,
    /// Maximum payload size (0 = unlimited).
    pub max_payload: usize,
}

impl Default for LengthConfig {
    fn default() -> Self {
        Self {
            len_bytes: 2,
            endian: Endian::Big,
            length_includes_self: false,
            max_payload: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
enum State {
    ReadingLength,
    ReadingPayload { expected: usize },
}

#[derive(Debug, Clone)]
pub struct LengthPrefixedFramer {
    buf: Vec<u8>,
    config: LengthConfig,
    state: State,
}

impl LengthPrefixedFramer {
    pub fn new(config: LengthConfig) -> Self {
        assert!(
            matches!(config.len_bytes, 1 | 2 | 4),
            "len_bytes must be 1, 2, or 4"
        );
        Self {
            buf: Vec::new(),
            config,
            state: State::ReadingLength,
        }
    }

    pub fn config(&self) -> &LengthConfig {
        &self.config
    }

    fn parse_length(&self, bytes: &[u8]) -> usize {
        let raw = match self.config.len_bytes {
            1 => bytes[0] as usize,
            2 => {
                let b = [bytes[0], bytes[1]];
                match self.config.endian {
                    Endian::Big => u16::from_be_bytes(b) as usize,
                    Endian::Little => u16::from_le_bytes(b) as usize,
                }
            }
            4 => {
                let b = [bytes[0], bytes[1], bytes[2], bytes[3]];
                match self.config.endian {
                    Endian::Big => u32::from_be_bytes(b) as usize,
                    Endian::Little => u32::from_le_bytes(b) as usize,
                }
            }
            _ => unreachable!(),
        };

        if self.config.length_includes_self {
            raw.saturating_sub(self.config.len_bytes)
        } else {
            raw
        }
    }
}

impl Framer for LengthPrefixedFramer {
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        self.buf.extend_from_slice(data);

        loop {
            match self.state {
                State::ReadingLength => {
                    if self.buf.len() < self.config.len_bytes {
                        break;
                    }
                    let payload_len = self.parse_length(&self.buf[..self.config.len_bytes]);

                    // Discard length, re-sync on corrupt frame
                    if self.config.max_payload > 0 && payload_len > self.config.max_payload {
                        self.buf.drain(..self.config.len_bytes);
                        continue;
                    }

                    self.buf.drain(..self.config.len_bytes);

                    if payload_len == 0 {
                        frames.push(Vec::new());
                    } else {
                        self.state = State::ReadingPayload {
                            expected: payload_len,
                        };
                    }
                }
                State::ReadingPayload { expected } => {
                    if self.buf.len() < expected {
                        break;
                    }
                    let frame: Vec<u8> = self.buf.drain(..expected).collect();
                    frames.push(frame);
                    self.state = State::ReadingLength;
                }
            }
        }

        frames
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        let remainder = std::mem::take(&mut self.buf);
        self.state = State::ReadingLength;
        if remainder.is_empty() {
            None
        } else {
            Some(remainder)
        }
    }

    fn reset(&mut self) {
        self.buf.clear();
        self.state = State::ReadingLength;
    }

    fn pending_len(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be2(len: u16) -> Vec<u8> {
        len.to_be_bytes().to_vec()
    }

    fn le2(len: u16) -> Vec<u8> {
        len.to_le_bytes().to_vec()
    }

    // ── 2-byte BE, length excludes self ───────────────────────────────

    #[test]
    fn length_single_frame() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        let mut data = be2(5);
        data.extend_from_slice(b"hello");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn length_two_frames_in_one_chunk() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        let mut data = Vec::new();
        data.extend(&be2(3));
        data.extend_from_slice(b"foo");
        data.extend(&be2(3));
        data.extend_from_slice(b"bar");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"foo");
        assert_eq!(frames[1], b"bar");
    }

    #[test]
    fn length_split_header() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        // Feed first half of 2-byte length header
        assert!(f.feed(&be2(5)[..1]).is_empty());
        assert_eq!(f.pending_len(), 1);
        let mut rest = be2(5)[1..].to_vec();
        rest.extend_from_slice(b"hello");
        let frames = f.feed(&rest);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn length_split_payload() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        let frames = f.feed(&be2(5));
        assert!(frames.is_empty());
        assert_eq!(f.pending_len(), 0); // length consumed, waiting for payload

        let frames = f.feed(b"hel");
        assert!(frames.is_empty());

        let frames = f.feed(b"lo");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn length_zero_payload() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        let frames = f.feed(&be2(0));
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_empty());
    }

    #[test]
    fn length_chain_zero_then_data() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        let mut data = Vec::new();
        data.extend(&be2(0));
        data.extend(&be2(3));
        data.extend_from_slice(b"foo");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 2);
        assert!(frames[0].is_empty());
        assert_eq!(frames[1], b"foo");
    }

    // ── 2-byte LE ────────────────────────────────────────────────────

    #[test]
    fn length_little_endian() {
        let cfg = LengthConfig {
            endian: Endian::Little,
            ..LengthConfig::default()
        };
        let mut f = LengthPrefixedFramer::new(cfg);
        let mut data = le2(5);
        data.extend_from_slice(b"hello");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    // ── 1-byte length ────────────────────────────────────────────────

    #[test]
    fn length_one_byte() {
        let cfg = LengthConfig {
            len_bytes: 1,
            ..LengthConfig::default()
        };
        let mut f = LengthPrefixedFramer::new(cfg);
        let data = [5, b'h', b'e', b'l', b'l', b'o'];
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    // ── 4-byte length ────────────────────────────────────────────────

    #[test]
    fn length_four_byte() {
        let cfg = LengthConfig {
            len_bytes: 4,
            ..LengthConfig::default()
        };
        let mut f = LengthPrefixedFramer::new(cfg);
        let mut data = 5u32.to_be_bytes().to_vec();
        data.extend_from_slice(b"hello");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    // ── length_includes_self ─────────────────────────────────────────

    #[test]
    fn length_includes_self_enabled() {
        let cfg = LengthConfig {
            length_includes_self: true,
            ..LengthConfig::default()
        };
        let mut f = LengthPrefixedFramer::new(cfg);
        let mut data = be2(5);
        data.extend_from_slice(b"foo");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"foo");
    }

    #[test]
    fn length_includes_self_zero_payload() {
        let cfg = LengthConfig {
            length_includes_self: true,
            ..LengthConfig::default()
        };
        let mut f = LengthPrefixedFramer::new(cfg);
        let frames = f.feed(&be2(2));
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_empty());
    }

    // ── max_payload safety ───────────────────────────────────────────

    #[test]
    fn length_max_payload_exceeded_skips() {
        let cfg = LengthConfig {
            max_payload: 10,
            ..LengthConfig::default()
        };
        let mut f = LengthPrefixedFramer::new(cfg);
        // Corrupt frame: claims 200 bytes, actual payload is 0xFF bytes
        let mut data = be2(200);
        data.extend_from_slice(&vec![0xFFu8; 200]);
        // Followed by valid frame
        data.extend(&be2(3));
        data.extend_from_slice(b"foo");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"foo");
    }

    // ── flush / reset ────────────────────────────────────────────────

    #[test]
    fn length_flush_partial() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        f.feed(&be2(10));
        f.feed(b"abc");
        let flushed = f.flush();
        assert_eq!(flushed, Some(b"abc".to_vec()));
    }

    #[test]
    fn length_flush_empty() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        assert_eq!(f.flush(), None);
    }

    #[test]
    fn length_reset_mid_frame() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        f.feed(&be2(50));
        f.feed(b"partial");
        assert!(f.pending_len() > 0);
        f.reset();
        assert_eq!(f.pending_len(), 0);
        // Should work normally after reset
        let mut data = be2(3);
        data.extend_from_slice(b"foo");
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"foo");
    }

    #[test]
    fn length_pending_len_reflects_buffer() {
        let mut f = LengthPrefixedFramer::new(LengthConfig::default());
        f.feed(&be2(5));
        assert_eq!(f.pending_len(), 0);
        f.feed(b"he");
        assert_eq!(f.pending_len(), 2);
    }
}
