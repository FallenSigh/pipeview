use super::Framer;
use crate::frame::cobs::cobs_decode;
use crate::protocol::mixed::{
    MIXED_FRAME_KIND_PLOT, MIXED_FRAME_KIND_TEXT, MIXED_PLOT_ESCAPE, MIXED_PLOT_MARKER,
};

#[derive(Debug, Clone)]
pub struct MixedTextPlotConfig {
    pub strip_cr: bool,
    pub max_line_len: usize,
    pub max_plot_frame: usize,
}

impl Default for MixedTextPlotConfig {
    fn default() -> Self {
        Self {
            strip_cr: true,
            max_line_len: 1024 * 1024,
            max_plot_frame: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Text,
    Escape,
    Plot,
}

#[derive(Debug, Clone)]
pub struct MixedTextPlotFramer {
    state: State,
    text_buf: Vec<u8>,
    plot_buf: Vec<u8>,
    plot_overflow: bool,
    config: MixedTextPlotConfig,
}

impl MixedTextPlotFramer {
    pub fn new(config: MixedTextPlotConfig) -> Self {
        Self {
            state: State::Text,
            text_buf: Vec::new(),
            plot_buf: Vec::new(),
            plot_overflow: false,
            config,
        }
    }

    fn push_text_byte(&mut self, byte: u8, frames: &mut Vec<Vec<u8>>) {
        if byte == b'\n' {
            let mut line = std::mem::take(&mut self.text_buf);
            if self.config.strip_cr && line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() <= self.config.max_line_len {
                frames.push(tag_text_frame(line));
            }
            return;
        }

        if self.text_buf.len() < self.config.max_line_len.saturating_add(1) {
            self.text_buf.push(byte);
        }
    }

    fn finish_plot_frame(&mut self, frames: &mut Vec<Vec<u8>>) {
        if self.plot_overflow || self.plot_buf.is_empty() {
            self.plot_buf.clear();
            self.plot_overflow = false;
            return;
        }

        if let Some(decoded) = cobs_decode(&self.plot_buf) {
            if decoded.len() <= self.config.max_plot_frame {
                frames.push(tag_plot_frame(decoded));
            }
        }
        self.plot_buf.clear();
        self.plot_overflow = false;
    }
}

impl Framer for MixedTextPlotFramer {
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        for &byte in data {
            match self.state {
                State::Text => {
                    if byte == MIXED_PLOT_ESCAPE {
                        self.state = State::Escape;
                    } else {
                        self.push_text_byte(byte, &mut frames);
                    }
                }
                State::Escape => {
                    self.state = State::Text;
                    match byte {
                        MIXED_PLOT_ESCAPE => self.push_text_byte(MIXED_PLOT_ESCAPE, &mut frames),
                        MIXED_PLOT_MARKER => {
                            self.plot_buf.clear();
                            self.plot_overflow = false;
                            self.state = State::Plot;
                        }
                        _ => {
                            self.push_text_byte(MIXED_PLOT_ESCAPE, &mut frames);
                            self.push_text_byte(byte, &mut frames);
                        }
                    }
                }
                State::Plot => {
                    if byte == 0x00 {
                        self.finish_plot_frame(&mut frames);
                        self.state = State::Text;
                    } else if !self.plot_overflow {
                        let max_encoded_len = self
                            .config
                            .max_plot_frame
                            .saturating_add(self.config.max_plot_frame / 254)
                            .saturating_add(8);
                        if self.plot_buf.len() < max_encoded_len {
                            self.plot_buf.push(byte);
                        } else {
                            self.plot_overflow = true;
                        }
                    }
                }
            }
        }

        frames
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        if matches!(self.state, State::Escape) {
            self.text_buf.push(MIXED_PLOT_ESCAPE);
        }

        self.state = State::Text;
        self.plot_buf.clear();
        self.plot_overflow = false;

        if self.text_buf.is_empty() {
            None
        } else {
            Some(tag_text_frame(std::mem::take(&mut self.text_buf)))
        }
    }

    fn reset(&mut self) {
        self.state = State::Text;
        self.text_buf.clear();
        self.plot_buf.clear();
        self.plot_overflow = false;
    }

    fn pending_len(&self) -> usize {
        self.text_buf.len() + self.plot_buf.len()
    }
}

fn tag_text_frame(mut payload: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 1);
    frame.push(MIXED_FRAME_KIND_TEXT);
    frame.append(&mut payload);
    frame
}

fn tag_plot_frame(mut payload: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 1);
    frame.push(MIXED_FRAME_KIND_PLOT);
    frame.append(&mut payload);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::cobs::cobs_encode;

    #[test]
    fn mixed_text_lines_pass_through() {
        let mut framer = MixedTextPlotFramer::new(MixedTextPlotConfig::default());
        let frames = framer.feed(b"hello\nworld\n");
        assert_eq!(frames, vec![b"\0hello".to_vec(), b"\0world".to_vec()]);
    }

    #[test]
    fn mixed_escaped_rs_stays_in_text() {
        let mut framer = MixedTextPlotFramer::new(MixedTextPlotConfig::default());
        let frames = framer.feed(&[b'o', b'k', MIXED_PLOT_ESCAPE, MIXED_PLOT_ESCAPE, b'\n']);
        assert_eq!(frames, vec![vec![0x00, b'o', b'k', MIXED_PLOT_ESCAPE]]);
    }

    #[test]
    fn mixed_extracts_plot_packet() {
        let mut framer = MixedTextPlotFramer::new(MixedTextPlotConfig::default());
        let payload = [b'X', b'P', 1, 0, 8, 0, 1, 1, 0, 4, 0, 0, 0, 0, 0x80, 0x3f];
        let encoded = cobs_encode(&payload);
        let mut stream = vec![MIXED_PLOT_ESCAPE, MIXED_PLOT_MARKER];
        stream.extend_from_slice(&encoded);
        stream.push(0x00);
        let frames = framer.feed(&stream);
        assert_eq!(frames, vec![[vec![0x01], payload.to_vec()].concat()]);
    }

    #[test]
    fn mixed_ignores_truncated_plot_on_flush() {
        let mut framer = MixedTextPlotFramer::new(MixedTextPlotConfig::default());
        let _ = framer.feed(&[MIXED_PLOT_ESCAPE, MIXED_PLOT_MARKER, 0x03, b'a']);
        assert_eq!(framer.flush(), None);
    }
}
