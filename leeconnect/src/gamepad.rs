//! Virtual gamepad forwarding (UU feature: device_gamepad / pad_touch).
//!
//! - Controller side polls local Xbox controllers via Win32 XInput (`XInputGetState`).
//! - Host side dynamically loads `ViGEmClient.dll` (when ViGEmBus driver is installed)
//!   and injects received `GamepadState` into a virtual Xbox 360 controller.
//!   Degrades cleanly when ViGEmBus is absent without failing compilation or startup.

use anyhow::{bail, ensure, Result};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadState {
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub thumb_lx: i16,
    pub thumb_ly: i16,
    pub thumb_rx: i16,
    pub thumb_ry: i16,
}

impl GamepadState {
    pub const WIRE_LEN: usize = 12;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.extend_from_slice(&self.buttons.to_be_bytes());
        b.push(self.left_trigger);
        b.push(self.right_trigger);
        b.extend_from_slice(&self.thumb_lx.to_be_bytes());
        b.extend_from_slice(&self.thumb_ly.to_be_bytes());
        b.extend_from_slice(&self.thumb_rx.to_be_bytes());
        b.extend_from_slice(&self.thumb_ry.to_be_bytes());
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        ensure!(b.len() >= Self::WIRE_LEN, "short gamepad payload");
        Ok(Self {
            buttons: u16::from_be_bytes([b[0], b[1]]),
            left_trigger: b[2],
            right_trigger: b[3],
            thumb_lx: i16::from_be_bytes([b[4], b[5]]),
            thumb_ly: i16::from_be_bytes([b[6], b[7]]),
            thumb_rx: i16::from_be_bytes([b[8], b[9]]),
            thumb_ry: i16::from_be_bytes([b[10], b[11]]),
        })
    }
}

/// Poll controller `user_index` (0..3) via XInput. Returns `None` if disconnected.
pub fn poll_xinput(user_index: u32) -> Option<GamepadState> {
    use windows::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};
    if user_index >= 4 {
        return None;
    }
    let mut state = XINPUT_STATE::default();
    let res = unsafe { XInputGetState(user_index, &mut state) };
    if res != 0 {
        return None;
    }
    let gp = state.Gamepad;
    Some(GamepadState {
        buttons: gp.wButtons.0,
        left_trigger: gp.bLeftTrigger,
        right_trigger: gp.bRightTrigger,
        thumb_lx: gp.sThumbLX,
        thumb_ly: gp.sThumbLY,
        thumb_rx: gp.sThumbRX,
        thumb_ry: gp.sThumbRY,
    })
}

// ---------------------------------------------------------------------------
// Host ViGEmClient dynamic loader (zero link-time dependency on ViGEmClient.dll)
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
struct XusbReport {
    w_buttons: u16,
    b_left_trigger: u8,
    b_right_trigger: u8,
    s_thumb_lx: i16,
    s_thumb_ly: i16,
    s_thumb_rx: i16,
    s_thumb_ry: i16,
}

type FnAlloc = unsafe extern "C" fn() -> *mut c_void;
type FnFree = unsafe extern "C" fn(*mut c_void);
type FnConnect = unsafe extern "C" fn(*mut c_void) -> u32;
type FnDisconnect = unsafe extern "C" fn(*mut c_void);
type FnTargetAlloc = unsafe extern "C" fn() -> *mut c_void;
type FnTargetAdd = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type FnTargetRemove = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
type FnTargetUpdate = unsafe extern "C" fn(*mut c_void, *mut c_void, XusbReport) -> u32;

const VIGEM_ERROR_NONE: u32 = 0x2000_0000;

static PROBED_VIGEM: AtomicBool = AtomicBool::new(false);
static HAS_VIGEM: AtomicBool = AtomicBool::new(false);

/// Probe whether `ViGEmClient.dll` is present on PATH / system directory.
pub fn is_vigem_available() -> bool {
    if PROBED_VIGEM.load(Ordering::Relaxed) {
        return HAS_VIGEM.load(Ordering::Relaxed);
    }
    let ok = unsafe {
        use windows::core::w;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        LoadLibraryW(w!("ViGEmClient.dll")).is_ok()
    };
    HAS_VIGEM.store(ok, Ordering::Relaxed);
    PROBED_VIGEM.store(true, Ordering::Relaxed);
    ok
}

pub struct VigemInjector {
    client: *mut c_void,
    target: *mut c_void,
    fn_disconnect: FnDisconnect,
    fn_free: FnFree,
    fn_target_remove: FnTargetRemove,
    fn_target_free: FnFree,
    fn_target_update: FnTargetUpdate,
}

unsafe impl Send for VigemInjector {}

impl VigemInjector {
    pub fn open() -> Result<Self> {
        use windows::core::{s, w};
        use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

        unsafe {
            let hmod = LoadLibraryW(w!("ViGEmClient.dll"))
                .map_err(|_| anyhow::anyhow!("ViGEmClient.dll not found (ViGEmBus driver not installed)"))?;
            macro_rules! load_sym {
                ($name:expr, $ty:ty) => {{
                    let p = GetProcAddress(hmod, $name)
                        .ok_or_else(|| anyhow::anyhow!("missing symbol in ViGEmClient.dll"))?;
                    std::mem::transmute::<_, $ty>(p)
                }};
            }
            let fn_alloc = load_sym!(s!("vigem_alloc"), FnAlloc);
            let fn_free = load_sym!(s!("vigem_free"), FnFree);
            let fn_connect = load_sym!(s!("vigem_connect"), FnConnect);
            let fn_disconnect = load_sym!(s!("vigem_disconnect"), FnDisconnect);
            let fn_target_alloc = load_sym!(s!("vigem_target_x360_alloc"), FnTargetAlloc);
            let fn_target_free = load_sym!(s!("vigem_target_free"), FnFree);
            let fn_target_add = load_sym!(s!("vigem_target_add"), FnTargetAdd);
            let fn_target_remove = load_sym!(s!("vigem_target_remove"), FnTargetRemove);
            let fn_target_update = load_sym!(s!("vigem_target_x360_update"), FnTargetUpdate);

            let client = fn_alloc();
            if client.is_null() {
                bail!("vigem_alloc returned null");
            }
            let rc = fn_connect(client);
            if rc != VIGEM_ERROR_NONE {
                fn_free(client);
                bail!("vigem_connect failed: {rc:#010x} (ViGEmBus service running?)");
            }
            let target = fn_target_alloc();
            if target.is_null() {
                fn_disconnect(client);
                fn_free(client);
                bail!("vigem_target_x360_alloc returned null");
            }
            let rc2 = fn_target_add(client, target);
            if rc2 != VIGEM_ERROR_NONE {
                fn_target_free(target);
                fn_disconnect(client);
                fn_free(client);
                bail!("vigem_target_add failed: {rc2:#010x}");
            }
            Ok(Self {
                client,
                target,
                fn_disconnect,
                fn_free,
                fn_target_remove,
                fn_target_free,
                fn_target_update,
            })
        }
    }

    pub fn update(&mut self, gp: GamepadState) -> Result<()> {
        let rep = XusbReport {
            w_buttons: gp.buttons,
            b_left_trigger: gp.left_trigger,
            b_right_trigger: gp.right_trigger,
            s_thumb_lx: gp.thumb_lx,
            s_thumb_ly: gp.thumb_ly,
            s_thumb_rx: gp.thumb_rx,
            s_thumb_ry: gp.thumb_ry,
        };
        let rc = unsafe { (self.fn_target_update)(self.client, self.target, rep) };
        if rc != VIGEM_ERROR_NONE {
            bail!("vigem_target_x360_update failed: {rc:#010x}");
        }
        Ok(())
    }
}

impl Drop for VigemInjector {
    fn drop(&mut self) {
        unsafe {
            (self.fn_target_remove)(self.client, self.target);
            (self.fn_target_free)(self.target);
            (self.fn_disconnect)(self.client);
            (self.fn_free)(self.client);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamepad_wire_roundtrip() {
        let g = GamepadState {
            buttons: 0x1005,
            left_trigger: 120,
            right_trigger: 255,
            thumb_lx: -16384,
            thumb_ly: 32767,
            thumb_rx: 42,
            thumb_ry: -1,
        };
        let bytes = g.encode();
        assert_eq!(bytes.len(), GamepadState::WIRE_LEN);
        assert_eq!(GamepadState::decode(&bytes).unwrap(), g);
    }

    #[test]
    fn gamepad_decode_rejects_short() {
        assert!(GamepadState::decode(&[0; 5]).is_err());
    }

    #[test]
    fn xinput_invalid_index_returns_none() {
        assert!(poll_xinput(4).is_none());
    }
}
