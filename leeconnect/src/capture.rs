//! Screen capture via DXGI Desktop Duplication (output-duplication).
//! Standard zero-copy desktop capture path (same API family Parsec/OBS use).
//! Self-implemented; no third-party capture code copied.

use anyhow::{bail, Context, Result};
use windows::core::{HRESULT, Interface};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
    IDXGIResource, DXGI_OUTDUPL_FRAME_INFO, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT,
};

const S_FALSE: HRESULT = HRESULT(0x0000_0001);

pub struct DesktopDup {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: Option<IDXGIOutputDuplication>,
    width: u32,
    height: u32,
}

#[derive(Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// bytes per row of `pixels` (>= width*4, texture row pitch)
    pub stride: u32,
    /// BGRA8, `stride`-pitched
    pub pixels: Vec<u8>,
    /// dirty-region bounding box (x, y, w, h); None = full-frame update
    pub dirty: Option<(u32, u32, u32, u32)>,
}

impl DesktopDup {
    /// Bind to the primary output of the default adapter.
    pub fn new_primary() -> Result<Self> {
        Self::new_for_output(0)
    }

    /// Bind to output `idx` (see `monitor_count`). Each output supports exactly
    /// one live duplication at a time — one DesktopDup per monitor.
    pub fn new_for_output(idx: usize) -> Result<Self> {
        unsafe {
            // DPI-aware so GDI device caps report physical pixels, matching the
            // DXGI texture size (otherwise scaled desktops mismatch).
            let _ = windows::Win32::UI::WindowsAndMessaging::SetProcessDPIAware();

            let factory: IDXGIFactory1 = CreateDXGIFactory1().context("CreateDXGIFactory1")?;
            let adapter = factory.EnumAdapters1(0).context("EnumAdapters1(0)")?;
            let output = adapter
                .EnumOutputs(idx as u32)
                .map_err(|_| anyhow::anyhow!("EnumOutputs({idx}): no such display"))?;
            let output1: IDXGIOutput1 = output.cast().context("IDXGIOutput1 cast")?;

            let mut device_opt: Option<ID3D11Device> = None;
            let mut ctx_opt: Option<ID3D11DeviceContext> = None;
            let mut feature = D3D_FEATURE_LEVEL(D3D_FEATURE_LEVEL_11_0.0);
            D3D11CreateDevice(
                &adapter, // bind to the same adapter that owns the output
                D3D_DRIVER_TYPE_UNKNOWN, // rule: non-null pAdapter requires UNKNOWN
                HMODULE(std::ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device_opt),
                Some(&mut feature),
                Some(&mut ctx_opt),
            )
            .map_err(|e| anyhow::anyhow!("D3D11CreateDevice: {e} (code {:#010x})", e.code().0 as u32))?;
            let device = device_opt.context("null device")?;
            let context = ctx_opt.context("null context")?;

            // per-output physical size from the DXGI descriptor (not GDI primary)
            let (width, height) = output_resolution(&output)?;

            let duplication = output1
                .DuplicateOutput(&device)
                .map_err(|e| anyhow::anyhow!("DuplicateOutput failed ({e}); needs an interactive desktop session, not session-0/service context"))?;

            let mut this = Self { device, context, duplication: Some(duplication), width, height };
            this.prime()?; // flush the initial empty/black frame a fresh duplication returns
            Ok(this)
        }
    }

    /// A fresh output-duplication returns a stale/black first frame until the
    /// desktop actually changes. Flush a couple of acquisitions so the caller's
    /// first `acquire()` yields live pixels.
    pub fn prime(&mut self) -> Result<()> {
        for _ in 0..2 {
            let _ = self.acquire(30)?;
        }
        Ok(())
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Acquire the next frame. `timeout_ms == 0` polls; returns `Ok(None)` when
    /// nothing changed (S_FALSE / WAIT_TIMEOUT).
    pub fn acquire(&mut self, timeout_ms: u32) -> Result<Option<CapturedFrame>> {
        let dup = self.duplication.as_mut().context("duplication closed")?;
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;
            if let Err(e) = dup.AcquireNextFrame(timeout_ms, &mut info, &mut resource) {
                let code = e.code();
                if code == S_FALSE || code == DXGI_ERROR_WAIT_TIMEOUT {
                    return Ok(None);
                }
                if code == DXGI_ERROR_ACCESS_LOST {
                    dup.ReleaseFrame().ok();
                    self.duplication = None;
                    bail!("DXGI access lost - recreate DesktopDup (resolution change / secure desktop)");
                }
                return Err(anyhow::Error::from(e).context("AcquireNextFrame"));
            }
            // clone the COM handles (cheap refcount) so we don't double-borrow self
            let device = self.device.clone();
            let context = self.context.clone();
            let res = copy_staged(&device, &context, &info, resource.context("null resource")?);
            dup.ReleaseFrame().ok();
            Ok(Some(res?))
        }
    }
}

/// Copy the acquired desktop texture into a CPU-readable staging texture.
fn copy_staged(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    info: &DXGI_OUTDUPL_FRAME_INFO,
    resource: IDXGIResource,
) -> Result<CapturedFrame> {
    unsafe {
        let tex: ID3D11Texture2D = resource.cast().context("IDXGIResource -> texture")?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        tex.GetDesc(&mut desc);
        let (w, h) = (desc.Width, desc.Height);

        let mut sd = D3D11_TEXTURE2D_DESC::default();
        sd.Width = w;
        sd.Height = h;
        sd.MipLevels = 1;
        sd.ArraySize = 1;
        sd.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        sd.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
        sd.Usage = D3D11_USAGE_STAGING;
        sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut staged_opt: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&sd, None, Some(&mut staged_opt))
            .context("staging texture")?;
        let staged = staged_opt.context("null staged")?;

        context.CopyResource(&staged, &tex);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(&staged, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .context("Map staging")?;

        let stride = mapped.RowPitch as usize;
        let mut pixels = vec![0u8; stride * h as usize];
        let src = mapped.pData as *const u8;
        for y in 0..h as usize {
            std::ptr::copy_nonoverlapping(
                src.add(y * stride),
                pixels.as_mut_ptr().add(y * stride),
                (w as usize * 4).min(stride),
            );
        }
        context.Unmap(&staged, 0);

        Ok(CapturedFrame {
            width: w,
            height: h,
            stride: stride as u32,
            pixels,
            dirty: dirty_box(info),
        })
    }
}

/// Bounding box of dirty rects (M7 refinement: needs GetFrameDirtyRects while the
/// frame is held; full-frame copy is correct for now).
fn dirty_box(_info: &DXGI_OUTDUPL_FRAME_INFO) -> Option<(u32, u32, u32, u32)> {
    None
}

/// Enumerate attached outputs on adapter 0. Returns (count, [desc per index]).
pub fn monitor_count() -> Result<usize> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().context("CreateDXGIFactory1")?;
        let adapter = factory.EnumAdapters1(0).context("EnumAdapters1(0)")?;
        let mut n = 0usize;
        loop {
            match adapter.EnumOutputs(n as u32) {
                Ok(_) => n += 1,
                Err(_) => break,
            }
        }
        Ok(n)
    }
}

/// Desktop bounds of an output via `GetDesc` (physical pixels, DPI-independent).
fn output_resolution(output: &windows::Win32::Graphics::Dxgi::IDXGIOutput) -> Result<(u32, u32)> {
    let desc = unsafe { output.GetDesc() }.context("IDXGIOutput::GetDesc")?;
    let (w, h) = (
        desc.DesktopCoordinates.right - desc.DesktopCoordinates.left,
        desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top,
    );
    if w <= 0 || h <= 0 {
        bail!("zero output resolution");
    }
    Ok((w as u32, h as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only meaningful on a machine with an interactive desktop + GPU.
    #[test]
    #[ignore]
    fn capture_one_frame() {
        let _g = crate::DESKTOP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut d = DesktopDup::new_primary().expect("dup init");
        let (w, h) = d.size();
        println!("gdi desktop {w}x{h}");
        assert!(w > 0 && h > 0);
        let f = d.acquire(500).ok().flatten();
        if let Some(f) = f {
            println!("frame {}x{} stride {}", f.width, f.height, f.stride);
            assert_eq!(f.width, w);
            assert_eq!(f.height, h);
            assert!(f.pixels.len() >= f.stride as usize * h as usize);
        }
    }
}

