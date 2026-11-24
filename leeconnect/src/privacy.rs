//! Privacy screen (UU features: screen_privacy / screen_privacy_enhanced /
//! privacy_wallpaper). Host-side: a topmost full-virtual-screen black window +
//! low-level keyboard/mouse hooks that swallow PHYSICAL input while still
//! letting the controller's SendInput (LLMHF_INJECTED) through.
//!
//! Safety backdoor: 3 Escapes within 1.5s disarms the guard locally, so the
//! machine can never be locked out.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BLACK_BRUSH, GetStockObject, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
    PeekMessageW, PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, TranslateMessage, UnhookWindowsHookEx, CallNextHookEx, SetWindowsHookExW,
    CS_HREDRAW, CS_VREDRAW, GWL_EXSTYLE, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_REMOVE,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KILLFOCUS, WM_QUIT, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, HWND_TOPMOST,
    MSLLHOOKSTRUCT, LLMHF_INJECTED,
};

/// Pure decision: should this low-level event be swallowed (physical, guard on)?
/// `injected` = LLMHF_INJECTED bit; guard off => never block.
pub fn should_block(active: bool, injected: bool) -> bool {
    active && !injected
}

/// Pure decision: does this Esc press trigger the 3-in-1.5s backdoor?
/// Returns (new_count, new_first_ms, fired).
pub fn backdoor_fires(count: u32, first_ms: u32, now_ms: u32) -> (u32, u32, bool) {
    const WINDOW_MS: u32 = 1500;
    const NEED: u32 = 3;
    if count == 0 || now_ms.saturating_sub(first_ms) > WINDOW_MS {
        return (1, now_ms, false); // restart window
    }
    let c = count + 1;
    if c >= NEED {
        (0, 0, true) // fire + reset
    } else {
        (c, first_ms, false)
    }
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static ESC_COUNT: AtomicU32 = AtomicU32::new(0);
static ESC_FIRST: AtomicU32 = AtomicU32::new(0);
static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

fn now_ms() -> u32 {
    EPOCH.elapsed().as_millis().min(u32::MAX as u128) as u32
}

unsafe extern "system" fn kb_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let ki = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = (ki.flags.0 & LLMHF_INJECTED) != 0;
        // backdoor: count physical Esc presses regardless of swallow
        if !injected && wparam.0 as u32 == VK_ESCAPE.0 as u32 {
            let (c, f, fire) = backdoor_fires(
                ESC_COUNT.load(Ordering::Relaxed),
                ESC_FIRST.load(Ordering::Relaxed),
                now_ms(),
            );
            ESC_COUNT.store(c, Ordering::Relaxed);
            ESC_FIRST.store(f, Ordering::Relaxed);
            if fire {
                ACTIVE.store(false, Ordering::SeqCst);
                return LRESULT(0); // swallow the disarming Esc too
            }
        }
        if should_block(ACTIVE.load(Ordering::SeqCst), injected) {
            return LRESULT(1); // swallow
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let mi = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let injected = (mi.flags & LLMHF_INJECTED) != 0;
        if should_block(ACTIVE.load(Ordering::SeqCst), injected) {
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_KILLFOCUS {
        // keep the black cover on top and non-focus-stealing
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (WS_EX_TOPMOST | WS_EX_TOOLWINDOW).0 as isize);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, w, l)
    }
}

/// Handle to the guard thread's cover window (for stop() to post quit).
static HOOK_THREAD: std::sync::Mutex<Option<std::thread::JoinHandle<()>>> =
    std::sync::Mutex::new(None);

pub struct PrivacyGuard;

impl PrivacyGuard {
    /// Start the privacy screen: black topmost cover + input hooks on a dedicated thread.
    pub fn start() -> Result<Self> {
        if Self::is_active() {
            return Ok(Self);
        }
        ACTIVE.store(true, Ordering::SeqCst);
        ESC_COUNT.store(0, Ordering::Relaxed);
        let j = std::thread::spawn(run_thread);
        *HOOK_THREAD.lock().unwrap() = Some(j);
        Ok(Self)
    }

    pub fn is_active() -> bool {
        ACTIVE.load(Ordering::SeqCst)
    }

    /// Programmatic disarm (controller toggling it off).
    pub fn stop() {
        ACTIVE.store(false, Ordering::SeqCst);
        if let Some(j) = HOOK_THREAD.lock().unwrap().take() {
            let _ = j.join();
        }
    }
}

fn run_thread() {
    unsafe {
        let hmod = GetModuleHandleW(None)
            .map(|h| HINSTANCE(h.0))
            .unwrap_or(HINSTANCE(std::ptr::null_mut()));
        let cls = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hmod,
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            lpszClassName: w!("LeeConnectPrivacy"),
            ..Default::default()
        };
        if RegisterClassExW(&cls) == 0 && windows::core::Error::from_win32().code().0 as u32 != 1410 {
            // 1410 = ERROR_CLASS_ALREADY_EXISTS
            return;
        }
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let cw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let ch = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("LeeConnectPrivacy"),
            w!(""),
            WS_POPUP,
            x,
            y,
            cw,
            ch,
            None,
            None,
            hmod,
            None,
        ) {
            Ok(h) => h,
            Err(_) => {
                ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook), hmod, 0);
        let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), hmod, 0);

        let mut msg = MSG::default();
        while ACTIVE.load(Ordering::SeqCst) {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    ACTIVE.store(false, Ordering::SeqCst);
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }

        if let Ok(h) = kb {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Ok(h) = ms {
            let _ = UnhookWindowsHookEx(h);
        }
        let _ = DestroyWindow(hwnd);
        PostQuitMessage(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_only_physical_when_active() {
        assert!(should_block(true, false), "physical input blocked when active");
        assert!(!should_block(true, true), "injected (remote) input passes");
        assert!(!should_block(false, false), "guard off => nothing blocked");
        assert!(!should_block(false, true));
    }

    #[test]
    fn backdoor_three_esc_in_window() {
        let (c, f, fire) = backdoor_fires(0, 0, 1000);
        assert_eq!((c, fire), (1, false));
        let (c2, _, fire2) = backdoor_fires(c, f, 1600);
        assert_eq!((c2, fire2), (2, false));
        let (_, _, fire3) = backdoor_fires(c2, f, 2000);
        assert!(fire3);
    }

    #[test]
    fn backdoor_window_resets_on_gap() {
        let (c, f, _) = backdoor_fires(0, 0, 1000);
        let (c2, _, fire) = backdoor_fires(c, f, 5000); // >1500ms gap
        assert_eq!(c2, 1, "window restarts after gap");
        assert!(!fire);
    }

    #[test]
    fn backdoor_needs_exactly_three() {
        let (mut cnt, mut first, mut fired) = backdoor_fires(0, 0, 0);
        (cnt, first, fired) = backdoor_fires(cnt, first, 100);
        assert!(!fired);
        (cnt, first, fired) = backdoor_fires(cnt, first, 200);
        assert!(fired, "third press fires");
    }
}
