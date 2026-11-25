//! Optional Android/MuMu cloud-phone control probe (UU feature: mumu_control).
//! Dynamically checks `nemu-api.dll` (MuMu Player 12 external renderer/input API)
//! without creating any build-time dependency on MuMu installation.

use std::ffi::c_void;

pub fn is_mumu_available() -> bool {
    unsafe {
        use windows::core::w;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        LoadLibraryW(w!("nemu-api.dll")).is_ok()
    }
}

pub struct MumuHandle {
    _raw: *mut c_void,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic() {
        let _ = is_mumu_available();
    }
}
