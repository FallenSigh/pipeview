use super::{DecodedData, ProtocolDecoder};

/// Supported text encodings for the text protocol decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    /// Standard UTF-8.  Returns `None` from `decode()` on invalid byte sequences.
    Utf8,
    /// ISO-8859-1 / Latin-1.  Every byte maps 1:1 to a Unicode code point.
    /// This encoding never fails.
    Latin1,
    /// 7-bit ASCII.  Returns `None` from `decode()` if any byte has the high bit set.
    Ascii,
}

/// Text protocol decoder.
///
/// Converts raw byte frames to UTF-8 strings using the configured encoding.
#[derive(Debug, Clone)]
pub struct TextDecoder {
    encoding: TextEncoding,
}

impl TextDecoder {
    pub fn new(encoding: TextEncoding) -> Self {
        Self { encoding }
    }

    pub fn encoding(&self) -> TextEncoding {
        self.encoding
    }
}

impl ProtocolDecoder for TextDecoder {
    fn name(&self) -> &str {
        "Text"
    }

    fn decode(&self, frame: &[u8]) -> Option<DecodedData> {
        let text = match self.encoding {
            TextEncoding::Utf8 => String::from_utf8(frame.to_vec()).ok()?,
            TextEncoding::Latin1 => frame.iter().map(|&b| b as char).collect(),
            TextEncoding::Ascii => {
                if frame.iter().any(|&b| b >= 128) {
                    return None;
                }
                frame.iter().map(|&b| b as char).collect()
            }
        };
        Some(DecodedData::Text(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_utf8_valid() {
        let d = TextDecoder::new(TextEncoding::Utf8);
        let result = d.decode(b"hello");
        assert!(matches!(result, Some(DecodedData::Text(ref s)) if s == "hello"));
    }

    #[test]
    fn text_utf8_chinese() {
        let d = TextDecoder::new(TextEncoding::Utf8);
        let result = d.decode("你好世界".as_bytes());
        assert!(matches!(result, Some(DecodedData::Text(ref s)) if s == "你好世界"));
    }

    #[test]
    fn text_utf8_invalid_returns_none() {
        let d = TextDecoder::new(TextEncoding::Utf8);
        let invalid = vec![0xff, 0xfe, 0xfd];
        assert!(d.decode(&invalid).is_none());
    }

    #[test]
    fn text_utf8_empty() {
        let d = TextDecoder::new(TextEncoding::Utf8);
        assert!(matches!(d.decode(b""), Some(DecodedData::Text(ref s)) if s.is_empty()));
    }

    #[test]
    fn text_latin1_all_bytes() {
        let d = TextDecoder::new(TextEncoding::Latin1);
        let data: Vec<u8> = (0..=255).collect();
        let result = d.decode(&data).unwrap();
        if let DecodedData::Text(s) = result {
            assert_eq!(s.chars().count(), 256);
            // Verify character code points match original bytes
            for (i, ch) in s.chars().enumerate() {
                assert_eq!(ch as u32, i as u32);
            }
        } else {
            panic!("expected Text");
        }
    }

    #[test]
    fn text_latin1_empty() {
        let d = TextDecoder::new(TextEncoding::Latin1);
        assert!(matches!(d.decode(b""), Some(DecodedData::Text(ref s)) if s.is_empty()));
    }

    #[test]
    fn text_ascii_valid() {
        let d = TextDecoder::new(TextEncoding::Ascii);
        let result = d.decode(b"Hello World 123!");
        assert!(matches!(result, Some(DecodedData::Text(ref s)) if s == "Hello World 123!"));
    }

    #[test]
    fn text_ascii_high_bit_returns_none() {
        let d = TextDecoder::new(TextEncoding::Ascii);
        assert!(d.decode(&[0x80]).is_none());
        assert!(d.decode(&[b'A', 0xff, b'B']).is_none());
    }

    #[test]
    fn text_ascii_boundary() {
        let d = TextDecoder::new(TextEncoding::Ascii);
        assert!(d.decode(&[0x7f]).is_some());
        assert!(d.decode(&[0x80]).is_none());
    }

    #[test]
    fn text_name() {
        assert_eq!(TextDecoder::new(TextEncoding::Utf8).name(), "Text");
        assert_eq!(TextDecoder::new(TextEncoding::Latin1).name(), "Text");
        assert_eq!(TextDecoder::new(TextEncoding::Ascii).name(), "Text");
    }

    #[test]
    fn text_encoding_accessor() {
        let d = TextDecoder::new(TextEncoding::Latin1);
        assert_eq!(d.encoding(), TextEncoding::Latin1);
    }

    #[test]
    fn text_decoder_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TextDecoder>();
    }
}
