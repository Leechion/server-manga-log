//! Follow-mode display resolution management (UU feature: follow_local / follow_remote).
//! Uses Win32 `EnumDisplaySettingsW` / `ChangeDisplaySettingsExW` with `CDS_TEST`
//! verification and RAII restoration on session end.

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub hz: u32,
}

/// Enumerate supported display modes on the primary desktop.
pub fn enumerate_modes() -> Vec<DisplayMode> {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_DISPLAY_SETTINGS_MODE};
    let mut modes = Vec::new();
    let mut idx = 0u32;
    unsafe {
        loop {
            let mut dm = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            if !EnumDisplaySettingsW(None, ENUM_DISPLAY_SETTINGS_MODE(idx), &mut dm).as_bool() {
                break;
            }
            if dm.dmBitsPerPel >= 24 && dm.dmPelsWidth > 0 && dm.dmPelsHeight > 0 {
                let m = DisplayMode {
                    width: dm.dmPelsWidth,
                    height: dm.dmPelsHeight,
                    hz: dm.dmDisplayFrequency,
                };
                if !modes.contains(&m) {
                    modes.push(m);
                }
            }
            idx += 1;
        }
    }
    modes
}

/// Pure selector: find the mode closest in area/aspect to `(target_w, target_h)`,
/// breaking ties by highest refresh rate.
pub fn pick_best_mode(modes: &[DisplayMode], target_w: u32, target_h: u32) -> Option<DisplayMode> {
    if modes.is_empty() || target_w == 0 || target_h == 0 {
        return None;
    }
    let target_aspect = target_w as f64 / target_h as f64;
    modes
        .iter()
        .copied()
        .min_by(|a, b| {
            let score = |m: &DisplayMode| -> i64 {
                let dw = (m.width as i64 - target_w as i64).abs();
                let dh = (m.height as i64 - target_h as i64).abs();
                let aspect_diff = ((m.width as f64 / m.height.max(1) as f64) - target_aspect).abs();
                (dw + dh) * 10 + (aspect_diff * 5000.0) as i64 - (m.hz as i64 / 10)
            };
            score(a).cmp(&score(b))
        })
}

/// Query current primary display resolution.
pub fn current_mode() -> Result<DisplayMode> {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    unsafe {
        let mut dm = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        if !EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut dm).as_bool() {
            bail!("EnumDisplaySettingsW(ENUM_CURRENT_SETTINGS) failed");
        }
        Ok(DisplayMode {
            width: dm.dmPelsWidth,
            height: dm.dmPelsHeight,
            hz: dm.dmDisplayFrequency,
        })
    }
}

/// Apply `(w, h)` safely (`CDS_TEST` validation before actual switch).
pub fn apply_mode(width: u32, height: u32) -> Result<DisplayMode> {
    use windows::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, DEVMODEW, DM_PELSWIDTH, DM_PELSHEIGHT, CDS_TEST,
        DISP_CHANGE_SUCCESSFUL,
    };
    let modes = enumerate_modes();
    let best = pick_best_mode(&modes, width, height)
        .ok_or_else(|| anyhow::anyhow!("no matching display mode for {width}x{height}"))?;

    unsafe {
        let dm = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            dmFields: DM_PELSWIDTH | DM_PELSHEIGHT,
            dmPelsWidth: best.width,
            dmPelsHeight: best.height,
            ..Default::default()
        };
        let test_rc = ChangeDisplaySettingsExW(None, Some(&dm), None, CDS_TEST, None);
        if test_rc != DISP_CHANGE_SUCCESSFUL {
            bail!("CDS_TEST rejected {}x{}: rc={}", best.width, best.height, test_rc.0);
        }
        let rc = ChangeDisplaySettingsExW(None, Some(&dm), None, Default::default(), None);
        if rc != DISP_CHANGE_SUCCESSFUL {
            bail!("ChangeDisplaySettingsExW failed: rc={}", rc.0);
        }
    }
    Ok(best)
}

/// Restores the display mode captured at session start when dropped.
pub struct DisplayRestoreGuard {
    original: Option<DisplayMode>,
}

impl DisplayRestoreGuard {
    pub fn capture() -> Self {
        Self { original: current_mode().ok() }
    }

    pub fn restore(&mut self) {
        if let Some(orig) = self.original.take() {
            let _ = apply_mode(orig.width, orig.height);
        }
    }
}

impl Drop for DisplayRestoreGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_best_mode_matches_exact_and_nearest() {
        let modes = vec![
            DisplayMode { width: 1280, height: 720, hz: 60 },
            DisplayMode { width: 1920, height: 1080, hz: 60 },
            DisplayMode { width: 1920, height: 1080, hz: 144 },
            DisplayMode { width: 2560, height: 1440, hz: 144 },
        ];
        let m = pick_best_mode(&modes, 1920, 1080).unwrap();
        assert_eq!((m.width, m.height, m.hz), (1920, 1080, 144));

        let m2 = pick_best_mode(&modes, 1300, 730).unwrap();
        assert_eq!((m2.width, m2.height), (1280, 720));
    }

    #[test]
    fn enumerate_and_current_mode_succeeds() {
        let cur = current_mode().expect("current display mode");
        assert!(cur.width > 0 && cur.height > 0);
        let all = enumerate_modes();
        assert!(!all.is_empty());
    }
}
