//! Binary frame protocol (self-designed). One header + payload, used over a
//! QUIC stream or the UDP relay. Big-endian on the wire.
//!
//! ```text
//! magic(4) version(1) type(1) channel(2) seq(4) flags(1) len(4) | payload(len)
//! ```

use thiserror::Error;

pub const MAGIC: u32 = 0x4C45_4543; // "LEEC"
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Video = 0,
    Input = 1,
    Clipboard = 2,
    File = 3,
    Control = 4,
    Audio = 5,
    Cursor = 6,
    Stats = 7,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Video,
            1 => Self::Input,
            2 => Self::Clipboard,
            3 => Self::File,
            4 => Self::Control,
            5 => Self::Audio,
            6 => Self::Cursor,
            7 => Self::Stats,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub ftype: FrameType,
    pub channel: u16,
    pub seq: u32,
    pub flags: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("buffer too short for header: have {have}, need {need}")]
    TooShort { have: usize, need: usize },
    #[error("bad magic: {0:#010x}")]
    BadMagic(u32),
    #[error("unsupported version {0}")]
    BadVersion(u8),
    #[error("unknown frame type {0}")]
    UnknownType(u8),
    #[error("truncated payload: have {have}, need {need}")]
    Truncated { have: usize, need: usize },
}

impl Frame {
    pub fn new(ftype: FrameType, channel: u16, seq: u32, payload: Vec<u8>) -> Self {
        Self { ftype, channel, seq, flags: 0, payload }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER_LEN + self.payload.len());
        b.extend_from_slice(&MAGIC.to_be_bytes());
        b.push(VERSION);
        b.push(self.ftype as u8);
        b.extend_from_slice(&self.channel.to_be_bytes());
        b.extend_from_slice(&self.seq.to_be_bytes());
        b.push(self.flags);
        b.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        b.extend_from_slice(&self.payload);
        b
    }

    /// Parse one frame; returns it plus how many bytes were consumed.
    pub fn decode(buf: &[u8]) -> Result<(Frame, usize), FrameError> {
        if buf.len() < HEADER_LEN {
            return Err(FrameError::TooShort { have: buf.len(), need: HEADER_LEN });
        }
        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC {
            return Err(FrameError::BadMagic(magic));
        }
        if buf[4] != VERSION {
            return Err(FrameError::BadVersion(buf[4]));
        }
        let ftype = FrameType::from_u8(buf[5]).ok_or(FrameError::UnknownType(buf[5]))?;
        let channel = u16::from_be_bytes([buf[6], buf[7]]);
        let seq = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let flags = buf[12];
        let len = u32::from_be_bytes([buf[13], buf[14], buf[15], buf[16]]) as usize;
        let total = HEADER_LEN + len;
        if buf.len() < total {
            return Err(FrameError::Truncated { have: buf.len(), need: total });
        }
        Ok((Frame { ftype, channel, seq, flags, payload: buf[HEADER_LEN..total].to_vec() }, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let f = Frame::new(FrameType::Input, 7, 999, vec![1, 2, 3, 4, 5]);
        let b = f.encode();
        assert_eq!(b.len(), HEADER_LEN + 5);
        let (g, used) = Frame::decode(&b).unwrap();
        assert_eq!(f, g);
        assert_eq!(used, b.len());
    }

    #[test]
    fn empty_payload() {
        let f = Frame::new(FrameType::Control, 0, 0, vec![]);
        let (g, _) = Frame::decode(&f.encode()).unwrap();
        assert_eq!(g, f);
    }

    #[test]
    fn bad_magic_rejected() {
        let b = [0u8; 20];
        assert!(matches!(Frame::decode(&b), Err(FrameError::BadMagic(_))));
    }

    #[test]
    fn truncated_payload_rejected() {
        let f = Frame::new(FrameType::Video, 0, 0, vec![0u8; 100]);
        let mut b = f.encode();
        b.truncate(20); // header says 100 payload bytes, only 3 present
        assert!(matches!(Frame::decode(&b), Err(FrameError::Truncated { .. })));
    }

    #[test]
    fn too_short() {
        let b = [0u8; 5];
        assert!(matches!(Frame::decode(&b), Err(FrameError::TooShort { .. })));
    }
}
