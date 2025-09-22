//! SLIP framing for Meshtastic serial API
//!
//! Meshtastic serial binary API frames are SLIP encoded Protobuf messages.
//! We implement a simple incremental decoder and encoder.

pub const END: u8 = 0xC0; // frame delimiter
pub const ESC: u8 = 0xDB;
pub const ESC_END: u8 = 0xDC;
pub const ESC_ESC: u8 = 0xDD;

#[derive(Debug, Default)]
pub struct SlipDecoder {
    buf: Vec<u8>,
    esc: bool,
}

impl SlipDecoder {
    pub fn new() -> Self { Self { buf: Vec::new(), esc: false } }

    /// Push bytes, returning any completed frames.
    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &b in data {
            if self.esc {
                match b {
                    ESC_END => self.buf.push(END),
                    ESC_ESC => self.buf.push(ESC),
                    _ => { /* invalid escape - drop? */ }
                }
                self.esc = false;
                continue;
            }
            match b {
                END => {
                    if !self.buf.is_empty() { frames.push(std::mem::take(&mut self.buf)); }
                }
                ESC => { self.esc = true; }
                _ => self.buf.push(b),
            }
        }
        frames
    }
}

#[allow(dead_code)]
pub fn slip_encode(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(END); // start delimiter (optional, ensures clean boundary)
    for &b in payload {
        match b {
            END => { out.push(ESC); out.push(ESC_END); }
            ESC => { out.push(ESC); out.push(ESC_ESC); }
            _ => out.push(b)
        }
    }
    out.push(END);
    out
}
