//! Cursor state wire format (UU feature: mouse_cursor / smart_mouse).
//! Host polls the system cursor and streams position; the controller overlays
//! a marker when "show remote cursor" is on. Shape bitmaps are a later refinement.
//!
//! Wire (big-endian): `visible(1) x(i32) y(i32)` = 9 bytes.

use thiserror::Error;

pub const WIRE_LEN: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Error)]
pub enum CursorError {
    #[error("cursor payload too short: have {have}, need {need}")]
    TooShort { have: usize, need: usize },
}

impl CursorState {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(WIRE_LEN);
        b.push(self.visible as u8);
        b.extend_from_slice(&self.x.to_be_bytes());
        b.extend_from_slice(&self.y.to_be_bytes());
        b
    }

    pub fn decode(buf: &[u8]) -> Result<Self, CursorError> {
        if buf.len() < WIRE_LEN {
            return Err(CursorError::TooShort { have: buf.len(), need: WIRE_LEN });
        }
        Ok(Self {
            visible: buf[0] != 0,
            x: i32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]),
            y: i32::from_be_bytes([buf[5], buf[6], buf[7], buf[8]]),
        })
    }
}

/// Poll the current system cursor (position in screen pixels + visibility).
pub fn poll() -> CursorState {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetCursorInfo, CURSOR_SHOWING, CURSORINFO,
        };
        unsafe {
            let mut ci = CURSORINFO::default();
            ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
            match GetCursorInfo(&mut ci as *mut CURSORINFO) {
                Ok(()) => CursorState {
                    visible: ci.flags == CURSOR_SHOWING,
                    x: ci.ptScreenPos.x,
                    y: ci.ptScreenPos.y,
                },
                Err(_) => CursorState { visible: false, x: 0, y: 0 },
            }
        }
    }
    #[cfg(not(windows))]
    {
        CursorState { visible: false, x: 0, y: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in [
            CursorState { visible: true, x: 1234, y: -5678 },
            CursorState { visible: false, x: 0, y: 0 },
            CursorState { visible: true, x: i32::MIN, y: i32::MAX },
        ] {
            let b = s.encode();
            assert_eq!(b.len(), WIRE_LEN);
            assert_eq!(CursorState::decode(&b).unwrap(), s);
        }
    }

    #[test]
    fn truncated_rejected() {
        assert!(matches!(
            CursorState::decode(&[1, 0, 0]),
            Err(CursorError::TooShort { .. })
        ));
    }

    #[test]
    fn visible_flag_strict() {
        // any non-zero first byte means visible
        let mut b = CursorState { visible: false, x: 9, y: 9 }.encode();
        b[0] = 2;
        assert!(CursorState::decode(&b).unwrap().visible);
    }

    #[test]
    fn poll_returns_plausible_state() {
        // on a headless/service session this may report invisible; must not panic
        let s = poll();
        let _ = s.encode();
    }
}
