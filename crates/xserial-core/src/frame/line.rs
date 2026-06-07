use super::Framer;

/// Line-delimited framer.
///
/// Splits incoming bytes on `\n` (LF).  Optional `\r` (CR) stripping is
/// controlled via [`LineConfig::strip_cr`].
#[derive(Debug, Clone)]
pub struct LineConfig {
    pub strip_cr: bool,
    pub max_line_len: usize,
}

impl Default for LineConfig {
    fn default() -> Self {
        Self {
            strip_cr: true,
            max_line_len: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LineFramer {
    buf: Vec<u8>,
    config: LineConfig,
}

impl LineFramer {
    pub fn new(config: LineConfig) -> Self {
        Self {
            buf: Vec::new(),
            config,
        }
    }

    pub fn config(&self) -> &LineConfig {
        &self.config
    }
}

impl Framer for LineFramer {
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        self.buf.extend_from_slice(data);

        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line = self.buf.drain(..=pos).collect::<Vec<u8>>();
            line.pop();
            if self.config.strip_cr && line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() <= self.config.max_line_len {
                frames.push(line);
            }
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
    fn line_single_complete() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"hello\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn line_multiple_in_one_chunk() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"aaa\nbbb\nccc\n");
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0], b"aaa");
        assert_eq!(frames[1], b"bbb");
        assert_eq!(frames[2], b"ccc");
    }

    #[test]
    fn line_split_across_chunks() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"hel");
        assert!(frames.is_empty());
        let frames = f.feed(b"lo\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello");
    }

    #[test]
    fn line_crlf_stripping() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"hello\r\nworld\r\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"hello");
        assert_eq!(frames[1], b"world");
    }

    #[test]
    fn line_crlf_no_strip() {
        let cfg = LineConfig {
            strip_cr: false,
            ..LineConfig::default()
        };
        let mut f = LineFramer::new(cfg);
        let frames = f.feed(b"hello\r\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"hello\r");
    }

    #[test]
    fn line_empty_lines() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"\n\n\n");
        assert_eq!(frames.len(), 3);
        assert!(frames[0].is_empty());
        assert!(frames[1].is_empty());
        assert!(frames[2].is_empty());
    }

    #[test]
    fn line_no_newline_buffered() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"incomplete");
        assert!(frames.is_empty());
        assert_eq!(f.pending_len(), 10);
    }

    #[test]
    fn line_flush_partial() {
        let mut f = LineFramer::new(LineConfig::default());
        f.feed(b"partial data");
        let flushed = f.flush();
        assert_eq!(flushed, Some(b"partial data".to_vec()));
        assert_eq!(f.pending_len(), 0);
    }

    #[test]
    fn line_flush_empty() {
        let mut f = LineFramer::new(LineConfig::default());
        assert_eq!(f.flush(), None);
    }

    #[test]
    fn line_reset() {
        let mut f = LineFramer::new(LineConfig::default());
        f.feed(b"some data");
        assert!(f.pending_len() > 0);
        f.reset();
        assert_eq!(f.pending_len(), 0);
    }

    #[test]
    fn line_max_len_filtered() {
        let cfg = LineConfig {
            max_line_len: 5,
            ..LineConfig::default()
        };
        let mut f = LineFramer::new(cfg);
        let frames = f.feed(b"short\nvery long line\nok\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"short");
        assert_eq!(frames[1], b"ok");
    }

    #[test]
    fn line_feed_then_flush_chain() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"complete\npartial");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"complete");

        let flushed = f.flush();
        assert_eq!(flushed, Some(b"partial".to_vec()));

        let frames = f.feed(b"new\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0], b"new");
    }

    #[test]
    fn line_binary_data_with_embedded_newlines() {
        let mut f = LineFramer::new(LineConfig::default());
        let data = [0x00, 0x01, b'\n', 0xFF, 0xFE, b'\n', 0x7F];
        let frames = f.feed(&data);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], &[0x00, 0x01]);
        assert_eq!(frames[1], &[0xFF, 0xFE]);
    }

    #[test]
    fn line_lf_only_with_strip_cr() {
        let mut f = LineFramer::new(LineConfig::default());
        let frames = f.feed(b"line1\nline2\n");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"line1");
        assert_eq!(frames[1], b"line2");
    }
}
