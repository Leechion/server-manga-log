//! Input injection via Win32 SendInput (mouse + keyboard), self-implemented.
//! The controller captures events, serializes to InputEvent, sends over the
//! input channel; the host replays them here.

use anyhow::{Context, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, VIRTUAL_KEY,
};
use windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS;

/// Virtual screen size for absolute-coordinate normalization (0..65535).
pub struct Screen {
    pub width: u32,
    pub height: u32,
}

/// Wire-level input event (matches `frame::FrameType::Input` payloads).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    MoveAbs { x: u32, y: u32 },
    MoveRel { dx: i32, dy: i32 },
    Button { button: MouseButton, down: bool },
    Wheel { delta: i32, horizontal: bool },
    Key { vk: u16, down: bool },
    /// raw scancode (for keys VK can't express reliably, e.g. some layouts)
    Scan { scancode: u16, down: bool, extended: bool },
    /// Xbox 360 virtual controller state
    Gamepad(crate::gamepad::GamepadState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

/// Replays input events onto this machine.
pub struct Injector {
    screen: Screen,
}

impl Injector {
    pub fn new(screen: Screen) -> Self {
        Self { screen }
    }

    pub fn inject(&self, ev: InputEvent) -> Result<()> {
        let inputs: Vec<INPUT> = match ev {
            InputEvent::MoveAbs { x, y } => {
                // clamp to screen then normalize to 0..65535 of virtual desktop
                let x = x.min(self.screen.width.saturating_sub(1));
                let y = y.min(self.screen.height.saturating_sub(1));
                let nx = (x as u64 * 65535 / self.screen.width.max(1) as u64) as i32;
                let ny = (y as u64 * 65535 / self.screen.height.max(1) as u64) as i32;
                vec![mouse(nx, ny, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, 0)]
            }
            InputEvent::MoveRel { dx, dy } => vec![mouse(dx, dy, MOUSEEVENTF_MOVE, 0)],
            InputEvent::Button { button, down } => vec![mouse(0, 0, button_flag(button, down), x_data(button, down))],
            InputEvent::Wheel { delta, horizontal } => {
                let flag = if horizontal { MOUSEEVENTF_HWHEEL } else { MOUSEEVENTF_WHEEL };
                vec![mouse(0, 0, flag, delta as u32)]
            }
            InputEvent::Key { vk, down } => vec![key(vk, down, KEYBD_EVENT_FLAGS(0))],
            InputEvent::Scan { scancode, down, extended } => {
                let mut f = KEYEVENTF_SCANCODE;
                if extended {
                    f |= KEYBD_EVENT_FLAGS(0x0002); // KEYEVENTF_EXTENDEDKEY
                }
                vec![scankey(scancode, down, f)]
            }
            InputEvent::Gamepad(_) => {
                // Virtual gamepad events are routed to `gamepad::VigemInjector` by the host.
                return Ok(());
            }
        };
        let n = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if n as usize != inputs.len() {
            let err = windows::core::Error::from_win32();
            return Err(anyhow::anyhow!("SendInput wrote {n}/{} inputs: {err}", inputs.len()))
                .context("inject");
        }
        Ok(())
    }

    /// Convenience: move + click left at absolute (x,y).
    pub fn click_left_at(&self, x: u32, y: u32) -> Result<()> {
        self.inject(InputEvent::MoveAbs { x, y })?;
        self.inject(InputEvent::Button { button: MouseButton::Left, down: true })?;
        self.inject(InputEvent::Button { button: MouseButton::Left, down: false })
    }
}

fn mouse(dx: i32, dy: i32, flags: MOUSE_EVENT_FLAGS, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: windows::Win32::UI::Input::KeyboardAndMouse::MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key(vk: u16, down: bool, extra: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: windows::Win32::UI::Input::KeyboardAndMouse::KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if down { extra } else { extra | KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn scankey(sc: u16, down: bool, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: windows::Win32::UI::Input::KeyboardAndMouse::KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: sc,
                dwFlags: if down { flags } else { flags | KEYEVENTF_KEYUP },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn button_flag(b: MouseButton, down: bool) -> MOUSE_EVENT_FLAGS {
    use MouseButton::*;
    match (b, down) {
        (Left, true) => MOUSEEVENTF_LEFTDOWN,
        (Left, false) => MOUSEEVENTF_LEFTUP,
        (Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (Right, false) => MOUSEEVENTF_RIGHTUP,
        (Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (Middle, false) => MOUSEEVENTF_MIDDLEUP,
        (X1, true) | (X2, true) => MOUSEEVENTF_XDOWN,
        (X1, false) | (X2, false) => MOUSEEVENTF_XUP,
    }
}

fn x_data(b: MouseButton, _down: bool) -> u32 {
    use MouseButton::*;
    match b {
        X1 => 0x0001,
        X2 => 0x0002,
        _ => 0,
    }
}

/// Serialize an InputEvent to wire bytes (1B tag + payload).
pub fn encode_input(ev: InputEvent) -> Vec<u8> {
    let mut b = Vec::new();
    match ev {
        InputEvent::MoveAbs { x, y } => {
            b.push(0);
            b.extend_from_slice(&x.to_be_bytes());
            b.extend_from_slice(&y.to_be_bytes());
        }
        InputEvent::MoveRel { dx, dy } => {
            b.push(1);
            b.extend_from_slice(&dx.to_be_bytes());
            b.extend_from_slice(&dy.to_be_bytes());
        }
        InputEvent::Button { button, down } => {
            b.push(2);
            b.push(button_code(button));
            b.push(down as u8);
        }
        InputEvent::Wheel { delta, horizontal } => {
            b.push(3);
            b.extend_from_slice(&delta.to_be_bytes());
            b.push(horizontal as u8);
        }
        InputEvent::Key { vk, down } => {
            b.push(4);
            b.extend_from_slice(&vk.to_be_bytes());
            b.push(down as u8);
        }
        InputEvent::Scan { scancode, down, extended } => {
            b.push(5);
            b.extend_from_slice(&scancode.to_be_bytes());
            b.push(down as u8);
            b.push(extended as u8);
        }
        InputEvent::Gamepad(gp) => {
            b.push(6);
            b.extend_from_slice(&gp.encode());
        }
    }
    b
}

pub fn decode_input(b: &[u8]) -> Result<InputEvent> {
    anyhow::ensure!(!b.is_empty(), "empty input");
    Ok(match b[0] {
        0 => InputEvent::MoveAbs {
            x: u32::from_be_bytes([b[1], b[2], b[3], b[4]]),
            y: u32::from_be_bytes([b[5], b[6], b[7], b[8]]),
        },
        1 => InputEvent::MoveRel {
            dx: i32::from_be_bytes([b[1], b[2], b[3], b[4]]),
            dy: i32::from_be_bytes([b[5], b[6], b[7], b[8]]),
        },
        2 => InputEvent::Button {
            button: button_from(b[1])?,
            down: b[2] != 0,
        },
        3 => InputEvent::Wheel {
            delta: i32::from_be_bytes([b[1], b[2], b[3], b[4]]),
            horizontal: b[5] != 0,
        },
        4 => InputEvent::Key {
            vk: u16::from_be_bytes([b[1], b[2]]),
            down: b[3] != 0,
        },
        5 => InputEvent::Scan {
            scancode: u16::from_be_bytes([b[1], b[2]]),
            down: b[3] != 0,
            extended: b[4] != 0,
        },
        6 => InputEvent::Gamepad(crate::gamepad::GamepadState::decode(&b[1..])?),
        t => anyhow::bail!("unknown input tag {t}"),
    })
}

fn button_code(b: MouseButton) -> u8 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
        MouseButton::X1 => 3,
        MouseButton::X2 => 4,
    }
}

fn button_from(c: u8) -> Result<MouseButton> {
    Ok(match c {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::X1,
        4 => MouseButton::X2,
        c => anyhow::bail!("bad button {c}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_codec_roundtrip() {
        let cases = [
            InputEvent::MoveAbs { x: 1920, y: 1080 },
            InputEvent::MoveRel { dx: -5, dy: 12 },
            InputEvent::Button { button: MouseButton::Left, down: true },
            InputEvent::Button { button: MouseButton::X2, down: false },
            InputEvent::Wheel { delta: -120, horizontal: false },
            InputEvent::Key { vk: 0x41, down: true },
            InputEvent::Scan { scancode: 0x1d, down: false, extended: true },
        ];
        for c in cases {
            let bytes = encode_input(c);
            let back = decode_input(&bytes).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_input(&[]).is_err());
        assert!(decode_input(&[99, 0, 0]).is_err());
    }

    /// Real injection: move mouse by a tiny relative delta and confirm SendInput
    /// accepted it (returns 1). Needs an interactive desktop.
    #[test]
    #[ignore]
    fn inject_relative_move_accepted() {
        let inj = Injector::new(Screen { width: 1920, height: 1080 });
        inj.inject(InputEvent::MoveRel { dx: 0, dy: 0 }).expect("zero-move should inject");
    }
}
