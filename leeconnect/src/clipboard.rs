//! Clipboard sync over the data plane (UU feature: remote_clipboard_v1..v5).
//! Bidirectional: local clipboard changes are pushed; remote changes are applied.
//! Wire: FrameType::Clipboard payload = [1B kind][text utf8] (kind 0=text, 1=clear).

use crate::frame::{Frame, FrameType};
use anyhow::{bail, ensure, Context, Result};
use windows::Win32::Foundation::{GlobalFree, HGLOBAL, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// CF_UNICODETEXT as the u32 format id the clipboard data APIs expect.
const CF_TEXT_FMT: u32 = CF_UNICODETEXT.0 as u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardEvent {
    Text(String),
    Clear,
}

impl ClipboardEvent {
    pub fn to_payload(&self) -> Vec<u8> {
        match self {
            ClipboardEvent::Text(s) => {
                let mut b = Vec::with_capacity(1 + s.len());
                b.push(0);
                b.extend_from_slice(s.as_bytes());
                b
            }
            ClipboardEvent::Clear => vec![1],
        }
    }

    pub fn from_payload(b: &[u8]) -> Result<Self> {
        ensure!(!b.is_empty(), "empty clipboard payload");
        Ok(match b[0] {
            0 => ClipboardEvent::Text(String::from_utf8(b[1..].to_vec()).context("clip utf8")?),
            1 => ClipboardEvent::Clear,
            k => bail!("unknown clipboard kind {k}"),
        })
    }

    pub fn into_frame(&self, seq: u32) -> Frame {
        Frame::new(FrameType::Clipboard, 6, seq, self.to_payload())
    }
}

/// Read the local Windows clipboard as text (None if empty / not text / busy).
pub fn read_clipboard_text() -> Result<Option<String>> {
    unsafe {
        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
            return Ok(None); // another app holds it
        }
        let mut out = None;
        if IsClipboardFormatAvailable(CF_TEXT_FMT).is_ok() {
            if let Ok(handle) = GetClipboardData(CF_TEXT_FMT) {
                let hg = HGLOBAL(handle.0);
                let size = GlobalSize(hg);
                let p = GlobalLock(hg) as *const u16;
                if !p.is_null() && size >= 2 {
                    let units = size / 2;
                    let slice = std::slice::from_raw_parts(p, units);
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(units);
                    out = Some(String::from_utf16_lossy(&slice[..end]));
                }
                let _ = GlobalUnlock(hg);
            }
        }
        let _ = CloseClipboard();
        Ok(out)
    }
}

/// Write text to the local clipboard. On success the clipboard owns the block.
pub fn write_clipboard_text(text: &str) -> Result<()> {
    unsafe {
        if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
            bail!("OpenClipboard failed (busy)");
        }
        let result = (|| -> Result<()> {
            EmptyClipboard().map_err(|e| anyhow::anyhow!("EmptyClipboard: {e}"))?;
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = wide.len() * 2;
            let hmem = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes).context("GlobalAlloc")?;
            let dst = GlobalLock(hmem) as *mut u16;
            ensure!(!dst.is_null(), "GlobalLock null");
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            // GlobalUnlock returns FALSE (with NO_ERROR) when the block is still
            // locked; with a single lock this is FALSE only on real failure.
            let unlocked = unsafe { GlobalUnlock(hmem) };
            if unlocked.is_err() {
                let gle = windows::Win32::Foundation::GetLastError();
                if gle.0 != 0 {
                    bail!("GlobalUnlock: {unlocked:?} gle={gle:?}");
                }
            }
            // SetClipboardData takes ownership of the block on success.
            SetClipboardData(CF_TEXT_FMT, HANDLE(hmem.0)).map_err(|e| anyhow::anyhow!("SetClipboardData: {e}"))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Apply a received clipboard event to the local clipboard.
pub fn apply(ev: &ClipboardEvent) -> Result<()> {
    match ev {
        ClipboardEvent::Text(t) => write_clipboard_text(t),
        ClipboardEvent::Clear => {
            unsafe {
                if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
                    bail!("OpenClipboard failed (busy)");
                }
                let r = EmptyClipboard();
                let _ = CloseClipboard();
                r.map_err(|e| anyhow::anyhow!("EmptyClipboard: {e}"))
            }
        }
    }
}

/// Release a global block if SetClipboardData failed (ownership stays with us).
#[allow(dead_code)]
unsafe fn free_global(h: HGLOBAL) {
    let _ = GlobalFree(h);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_codec_roundtrip() {
        let cases = [
            ClipboardEvent::Text("hello 世界".into()),
            ClipboardEvent::Text(String::new()),
            ClipboardEvent::Clear,
        ];
        for c in cases {
            let back = ClipboardEvent::from_payload(&c.to_payload()).unwrap();
            assert_eq!(c, back);
        }
    }

    #[test]
    fn decode_rejects_unknown() {
        assert!(ClipboardEvent::from_payload(&[]).is_err());
        assert!(ClipboardEvent::from_payload(&[9, 0, 0]).is_err());
    }

    /// Real clipboard round-trip. Needs a window-station clipboard (interactive session).
    #[test]
    #[ignore]
    fn clipboard_write_read_back() {
        let probe = format!("leeconnect-{}", std::process::id());
        write_clipboard_text(&probe).expect("write clip");
        let got = read_clipboard_text().expect("read clip");
        assert_eq!(got.as_deref(), Some(probe.as_str()));
    }
}
