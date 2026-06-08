pub mod cobs;
pub mod fixed;
pub mod length;
pub mod line;
pub mod mixed;

pub use crate::protocol::Endian;

/// Stateful byte-stream framer.
///
/// A `Framer` accumulates raw bytes from a transport layer and yields
/// complete frames once a boundary is detected.  Each call to [`feed`]
/// may return zero, one, or many frames depending on how much data has
/// arrived.
///
/// [`feed`]: Framer::feed
pub trait Framer: Send {
    /// Feed a chunk of newly arrived bytes.
    ///
    /// Any complete frames that can be extracted are returned.  The
    /// framer keeps incomplete trailing data in its internal buffer so
    /// it can be completed on the next call.
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>>;

    /// Drain any remaining buffered data as a final frame.
    ///
    /// This is typically called when the transport disconnects or the
    /// user explicitly wants to flush a partial frame.
    fn flush(&mut self) -> Option<Vec<u8>>;

    /// Discard all buffered state and start fresh.
    fn reset(&mut self);

    /// Number of bytes currently buffered in the incomplete frame.
    fn pending_len(&self) -> usize;
}
