use super::Framer;

/// COBS (Consistent Overhead Byte Stuffing) framer.
///
/// Frames are delimited by `0x00`.  Everything between two `0x00` bytes
/// is a COBS-encoded packet.  The framer accumulates until a `0x00` is
/// seen, decodes the COBS data, and yields the original payload.
///
/// Reference: <https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing>
#[derive(Debug, Clone)]
pub struct CobsFramer {
    buf: Vec<u8>,
    max_frame: usize,
}

impl Default for CobsFramer {
    fn default() -> Self {
        Self {
            buf: Vec::new(),
            max_frame: 1024 * 1024,
        }
    }
}

impl CobsFramer {
    pub fn new(max_frame: usize) -> Self {
        Self {
            buf: Vec::new(),
            max_frame,
        }
    }
}

impl Framer for CobsFramer {
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        for &byte in data {
            if byte == 0x00 {
                if !self.buf.is_empty() {
                    // Decode failed (corrupt packet) — discard and continue
                    if let Some(decoded) = cobs_decode(&self.buf) {
                        frames.push(decoded);
                    }
                    self.buf.clear();
                }
                // Consecutive 0x00 bytes produce empty frames
                // (skip them — no payload to decode)
            } else {
                if self.buf.len() < self.max_frame {
                    self.buf.push(byte);
                }
                // Exceeding max_frame: drop silently (avoid OOM)
            }
        }

        frames
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        if self.buf.is_empty() {
            None
        } else {
            let data = std::mem::take(&mut self.buf);
            cobs_decode(&data).or(Some(data))
        }
    }

    fn reset(&mut self) {
        self.buf.clear();
    }

    fn pending_len(&self) -> usize {
        self.buf.len()
    }
}

/// Decode a COBS-encoded packet (without the trailing 0x00 delimiter).
///
/// Returns `None` if the encoded data is invalid (e.g. an overhead byte
/// points past the end of the buffer).
fn cobs_decode(encoded: &[u8]) -> Option<Vec<u8>> {
    if encoded.is_empty() {
        return Some(Vec::new());
    }

    let mut out = Vec::with_capacity(encoded.len());
    let mut pos = 0;

    while pos < encoded.len() {
        let code = encoded[pos] as usize;
        if code == 0 {
            // Invalid: overhead byte should never be 0
            return None;
        }
        pos += 1;

        if code > 1 {
            let copy_start = pos;
            let desired_end = pos + code - 1;
            if desired_end > encoded.len() {
                return None;
            }
            out.extend_from_slice(&encoded[copy_start..desired_end]);
            pos = desired_end;
        }

        // If the code byte was < 0xFF, the next byte (if any) in the
        // original stream was 0x00 → insert it.
        if code < 0xFF && pos < encoded.len() {
            out.push(0x00);
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── cobs_decode unit tests ──────────────────────────────────────

    #[test]
    fn cobs_decode_no_zeros() {
        let decoded = cobs_decode(&[0x06, 0x68, 0x65, 0x6C, 0x6C, 0x6F]).unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn cobs_decode_single_zero() {
        let decoded = cobs_decode(&[0x03, 0x11, 0x22, 0x02, 0x33]).unwrap();
        assert_eq!(decoded, vec![0x11, 0x22, 0x00, 0x33]);
    }

    #[test]
    fn cobs_decode_leading_zero() {
        let decoded = cobs_decode(&[0x01, 0x01, 0x01, 0x01]).unwrap();
        assert_eq!(decoded, vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn cobs_decode_all_zeros() {
        // Input: [0x00, 0x00, 0x00] → encoded: [0x01, 0x01, 0x01, 0x01]
        let decoded = cobs_decode(&[0x01, 0x01, 0x01, 0x01]).unwrap();
        assert_eq!(decoded, vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn cobs_decode_empty() {
        let decoded = cobs_decode(&[]).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn cobs_decode_invalid_zero_code() {
        assert!(cobs_decode(&[0x00, 0x01]).is_none());
    }

    #[test]
    fn cobs_decode_truncated() {
        // Claims 5 bytes follow but only 3 remain
        assert!(cobs_decode(&[0x06, 0x01, 0x02, 0x03]).is_none());
    }

    #[test]
    fn cobs_decode_full_254_block() {
        // 254 non-zero bytes
        let payload: Vec<u8> = (1u8..=254).collect();
        let mut encoded = vec![0xFF];
        encoded.extend_from_slice(&payload);
        let decoded = cobs_decode(&encoded).unwrap();
        assert_eq!(decoded, payload);
    }

    // ── CobsFramer integration tests ─────────────────────────────────

    #[test]
    fn cobs_framer_single_packet() {
        let mut f = CobsFramer::default();
        let frames = f.feed(&[0x06, b'h', b'e', b'l', b'l', b'o', 0x00]);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn cobs_framer_two_packets() {
        let mut f = CobsFramer::default();
        let mut data = vec![0x04, b'f', b'o', b'o', 0x00];
        data.extend_from_slice(&[0x04, b'b', b'a', b'r', 0x00]);
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"foo");
        assert_eq!(frames[1], b"bar");
    }

    #[test]
    fn cobs_framer_split_across_chunks() {
        let mut f = CobsFramer::default();
        let frames = f.feed(&[0x06, b'h', b'e']);
        assert!(frames.is_empty());

        let frames = f.feed(&[b'l', b'l', b'o', 0x00]);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn cobs_framer_consecutive_zeros() {
        let mut f = CobsFramer::default();
        let frames = f.feed(&[0x00, 0x00, 0x03, b'a', b'b', 0x00]);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"ab");
    }

    #[test]
    fn cobs_framer_corrupt_packet_discarded() {
        let mut f = CobsFramer::default();
        let frames = f.feed(&[0x06, 0x01, 0x02, 0x00]);
        assert!(frames.is_empty());
    }

    #[test]
    fn cobs_framer_flush_partial() {
        let mut f = CobsFramer::default();
        f.feed(&[0x03, b'h', b'e']);
        let flushed = f.flush().unwrap();
        assert_eq!(flushed, b"he");
    }

    #[test]
    fn cobs_framer_flush_empty() {
        let mut f = CobsFramer::default();
        assert_eq!(f.flush(), None);
    }

    #[test]
    fn cobs_framer_reset() {
        let mut f = CobsFramer::default();
        f.feed(&[0x05, b'h', b'e']);
        assert!(f.pending_len() > 0);
        f.reset();
        assert_eq!(f.pending_len(), 0);
    }

    #[test]
    fn cobs_framer_max_frame() {
        let mut f = CobsFramer::new(3);
        f.feed(&[0x05, b'a', b'b', b'c', b'd', b'e']);
        assert_eq!(f.pending_len(), 3);
    }

    #[test]
    fn cobs_framer_empty_payload_packet() {
        let mut f = CobsFramer::default();
        let frames = f.feed(&[0x01, 0x00]);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_empty());
    }

    #[test]
    fn cobs_framer_pending_len() {
        let mut f = CobsFramer::default();
        f.feed(&[0x05, b'h', b'e']);
        assert_eq!(f.pending_len(), 3);
    }
}
