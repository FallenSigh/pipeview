use super::Framer;

/// Fixed-length framer — every N bytes forms one frame.
#[derive(Debug, Clone)]
pub struct FixedLengthFramer {
    buf: Vec<u8>,
    frame_len: usize,
}

impl FixedLengthFramer {
    pub fn new(frame_len: usize) -> Self {
        assert!(frame_len > 0, "frame_len must be > 0");
        Self {
            buf: Vec::new(),
            frame_len,
        }
    }

    pub fn frame_len(&self) -> usize {
        self.frame_len
    }
}

impl Framer for FixedLengthFramer {
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        self.buf.extend_from_slice(data);

        while self.buf.len() >= self.frame_len {
            let frame: Vec<u8> = self.buf.drain(..self.frame_len).collect();
            frames.push(frame);
        }

        frames
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        if self.buf.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buf))
        }
    }

    fn reset(&mut self) {
        self.buf.clear();
    }

    fn pending_len(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_exact_one_frame() {
        let mut f = FixedLengthFramer::new(5);
        let frames = f.feed(b"hello");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn fixed_multiple_frames() {
        let mut f = FixedLengthFramer::new(3);
        let frames = f.feed(b"abcdefghi");
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], b"abc");
        assert_eq!(frames[1], b"def");
        assert_eq!(frames[2], b"ghi");
    }

    #[test]
    fn fixed_partial_buffered() {
        let mut f = FixedLengthFramer::new(5);
        let frames = f.feed(b"abc");
        assert!(frames.is_empty());
        assert_eq!(f.pending_len(), 3);

        let frames = f.feed(b"de");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"abcde");
    }

    #[test]
    fn fixed_more_than_one_frame_in_chunk() {
        let mut f = FixedLengthFramer::new(4);
        let frames = f.feed(b"abcdefghij"); // 10 bytes → 2 full + 2 pending
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"abcd");
        assert_eq!(frames[1], b"efgh");
        assert_eq!(f.pending_len(), 2);
    }

    #[test]
    fn fixed_flush_partial() {
        let mut f = FixedLengthFramer::new(5);
        f.feed(b"xy");
        let flushed = f.flush();
        assert_eq!(flushed, Some(b"xy".to_vec()));
    }

    #[test]
    fn fixed_flush_empty() {
        let mut f = FixedLengthFramer::new(3);
        f.feed(b"abc");
        assert_eq!(f.flush(), None);
    }

    #[test]
    fn fixed_reset() {
        let mut f = FixedLengthFramer::new(4);
        f.feed(b"ab");
        assert_eq!(f.pending_len(), 2);
        f.reset();
        assert_eq!(f.pending_len(), 0);
    }

    #[test]
    fn fixed_empty_feed() {
        let mut f = FixedLengthFramer::new(5);
        let frames = f.feed(b"");
        assert!(frames.is_empty());
    }

    #[test]
    fn fixed_exact_multiple_drains() {
        let mut f = FixedLengthFramer::new(2);
        let frames = f.feed(b"abcd");
        assert_eq!(frames.len(), 2);
        assert!(f.pending_len() == 0);
        let more = f.feed(b"ef");
        assert_eq!(more.len(), 1);
        assert_eq!(more[0], b"ef");
    }

    #[test]
    #[should_panic(expected = "frame_len must be > 0")]
    fn fixed_zero_frame_len_panics() {
        FixedLengthFramer::new(0);
    }

    #[test]
    fn fixed_frame_len_accessor() {
        let f = FixedLengthFramer::new(128);
        assert_eq!(f.frame_len(), 128);
    }
}
