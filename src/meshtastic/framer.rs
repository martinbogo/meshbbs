use bytes::{BytesMut, Buf};

/// Maximum allowed frame size (sane upper bound to avoid runaway allocation)
const MAX_FRAME_SIZE: usize = 64 * 1024; // 64 KB

/// Simple varint length-delimited protobuf framer.
/// Meshtastic ToRadio/FromRadio messages on the serial link are sent as:
///   <varint length><protobuf bytes>
/// where length is the number of bytes in the following protobuf message.
pub struct ProtoFramer {
    buf: BytesMut,
}

impl ProtoFramer {
    pub fn new() -> Self { Self { buf: BytesMut::with_capacity(4096) } }

    pub fn push(&mut self, data: &[u8]) { self.buf.extend_from_slice(data); }

    /// Attempt to extract next complete frame. Returns Some(frame_bytes) if a full
    /// frame is available, otherwise None. On malformed data (oversize or invalid
    /// varint) it will drop leading byte and continue (resynchronization attempt).
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        // Need at least 1 byte for varint.
        if self.buf.is_empty() { return None; }

        // Parse varint (max 10 bytes for 64-bit, but we also limit size).
        let mut len: usize = 0;
        let mut shift = 0u32;
        let mut varint_len = 0usize;
    for (_i, b) in self.buf.iter().enumerate() {
            varint_len += 1;
            let val = (b & 0x7F) as usize;
            len |= val << shift;
            if (b & 0x80) == 0 { // end of varint
                break;
            }
            shift += 7;
            if shift > 28 { // unreasonably large for these frames; guard early
                // Drop first byte and retry (resync)
                self.buf.advance(1);
                return None;
            }
        }
        // If varint not complete yet
        if (self.buf.len() < varint_len) || (varint_len == 0) { return None; }

        // Check if varint terminated properly
        if self.buf[varint_len - 1] & 0x80 != 0 { return None; }

        if len > MAX_FRAME_SIZE { // oversize -> drop first byte
            self.buf.advance(1);
            return None;
        }
        // Need full payload
        if self.buf.len() < varint_len + len { return None; }

        // Split off frame
        let _ = self.buf.split_to(varint_len); // discard length prefix
        let frame = self.buf.split_to(len).to_vec();
        Some(frame)
    }
}
