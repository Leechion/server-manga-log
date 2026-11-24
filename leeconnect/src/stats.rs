//! Per-segment latency stats (UU feature: delay_reason).
//! Host measures capture+encode durations locally; controller adds its own
//! decode duration and the QUIC RTT to render a latency breakdown. Absolute
//! clocks aren't shared, so every number is a single-machine delta — safe to sum.
//!
//! Wire (big-endian, 8 bytes): `capture_us(u32) encode_us(u32)`.

use thiserror::Error;

pub const WIRE_LEN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// microseconds spent in DXGI acquire for this frame
    pub capture_us: u32,
    /// microseconds spent in the encoder for this frame
    pub encode_us: u32,
}

#[derive(Debug, Error)]
pub enum StatsError {
    #[error("stats payload too short: have {have}, need {need}")]
    TooShort { have: usize, need: usize },
}

impl Stats {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(WIRE_LEN);
        b.extend_from_slice(&self.capture_us.to_be_bytes());
        b.extend_from_slice(&self.encode_us.to_be_bytes());
        b
    }

    pub fn decode(buf: &[u8]) -> Result<Self, StatsError> {
        if buf.len() < WIRE_LEN {
            return Err(StatsError::TooShort { have: buf.len(), need: WIRE_LEN });
        }
        Ok(Self {
            capture_us: u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            encode_us: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
        })
    }

    /// Host-side pipeline cost (capture + encode) in microseconds.
    pub fn host_us(&self) -> u32 {
        self.capture_us.saturating_add(self.encode_us)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in [
            Stats { capture_us: 1200, encode_us: 8000 },
            Stats::default(),
            Stats { capture_us: u32::MAX, encode_us: 1 },
        ] {
            let b = s.encode();
            assert_eq!(b.len(), WIRE_LEN);
            assert_eq!(Stats::decode(&b).unwrap(), s);
        }
    }

    #[test]
    fn truncated_rejected() {
        assert!(matches!(Stats::decode(&[0, 1, 2]), Err(StatsError::TooShort { .. })));
    }

    #[test]
    fn host_us_saturates() {
        let s = Stats { capture_us: u32::MAX, encode_us: u32::MAX };
        assert_eq!(s.host_us(), u32::MAX);
    }
}
