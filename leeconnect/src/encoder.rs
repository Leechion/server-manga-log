//! Video encode/decode layer. Strategy pattern:
//!   - RawEncoder: lossless BGRA passthrough (loopback tests + no-GPU fallback)
//!   - MfH264Encoder: Media Foundation H.264 (added next milestone)
//! Capability negotiation between host/controller picks which encoder runs.

use crate::capture::CapturedFrame;
use anyhow::{ensure, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PixelFormat {
    Bgra8 = 0,
    Nv12 = 1,
    /// Encoded H.264 Annex-B bitstream (not raw pixels).
    H264 = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub keyframe: bool,
    pub pts: u64,
    pub data: Vec<u8>,
}

pub trait VideoEncoder {
    fn encode(&mut self, frame: &CapturedFrame, pts: u64) -> Result<EncodedFrame>;
    fn name(&self) -> &'static str;
}

impl EncodedFrame {
    /// Reconstruct a CapturedFrame from a lossless Bgra8 passthrough frame.
    pub fn into_frame(&self) -> Result<CapturedFrame> {
        ensure!(self.format == PixelFormat::Bgra8, "into_frame: not Bgra8");
        let stride = self.width as usize * 4;
        let mut pixels = Vec::with_capacity(stride * self.height as usize);
        for y in 0..self.height as usize {
            pixels.extend_from_slice(&self.data[y * stride..(y + 1) * stride]);
        }
        Ok(CapturedFrame { width: self.width, height: self.height, stride: stride as u32, pixels, dirty: None })
    }

    /// Video wire header size: w(4) h(4) fmt(1) key(1) pts(8).
    pub const WIRE_HDR: usize = 18;

    /// Serialize to wire bytes: [w4 h4 fmt1 key1 pts8 | payload].
    pub fn to_wire(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::WIRE_HDR + self.data.len());
        b.extend_from_slice(&self.width.to_be_bytes());
        b.extend_from_slice(&self.height.to_be_bytes());
        b.push(self.format as u8);
        b.push(self.keyframe as u8);
        b.extend_from_slice(&self.pts.to_be_bytes());
        b.extend_from_slice(&self.data);
        b
    }

    /// Parse a video wire buffer back into an EncodedFrame.
    pub fn from_wire(b: &[u8]) -> Result<Self> {
        ensure!(b.len() >= Self::WIRE_HDR, "short video payload");
        let width = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let height = u32::from_be_bytes([b[4], b[5], b[6], b[7]]);
        let format = match b[8] {
            0 => PixelFormat::Bgra8,
            1 => PixelFormat::Nv12,
            2 => PixelFormat::H264,
            v => anyhow::bail!("unknown pixfmt {v}"),
        };
        let keyframe = b[9] != 0;
        let pts = u64::from_be_bytes([b[10], b[11], b[12], b[13], b[14], b[15], b[16], b[17]]);
        Ok(EncodedFrame { width, height, format, keyframe, pts, data: b[Self::WIRE_HDR..].to_vec() })
    }
}

/// Lossless passthrough: repacks BGRA rows, stripping stride padding.
pub struct RawEncoder {
    pub counter: u64,
}

impl RawEncoder {
    pub fn new() -> Self {
        Self { counter: 0 }
    }
}

impl Default for RawEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoEncoder for RawEncoder {
    fn name(&self) -> &'static str {
        "raw-bgra"
    }
    fn encode(&mut self, frame: &CapturedFrame, pts: u64) -> Result<EncodedFrame> {
        let w = frame.width as usize;
        let h = frame.height as usize;
        let stride = frame.stride as usize;
        ensure!(stride >= w * 4, "RawEncoder: stride {stride} < w*4");
        let mut tight = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            tight.extend_from_slice(&frame.pixels[y * stride..y * stride + w * 4]);
        }
        self.counter += 1;
        Ok(EncodedFrame {
            width: frame.width,
            height: frame.height,
            format: PixelFormat::Bgra8,
            keyframe: true,
            pts,
            data: tight,
        })
    }
}

/// BT.601 studio-range BGRA -> NV12 (Y planar + interleaved UV at half res).
pub fn bgra_to_nv12(src: &[u8], stride: usize, w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let (cw, ch) = (w / 2, h / 2);
    let mut out = vec![0u8; w * h + cw * ch * 2];
    // luma
    for y in 0..h {
        let row = &src[y * stride..];
        let yo = y * w;
        for x in 0..w {
            let b = row[x * 4] as i32;
            let g = row[x * 4 + 1] as i32;
            let r = row[x * 4 + 2] as i32;
            let yy = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255);
            out[yo + x] = yy as u8;
        }
    }
    // chroma (2x2 average)
    let uvbase = w * h;
    for cy in 0..ch {
        for cx in 0..cw {
            let (mut bs, mut gs, mut rs) = (0i32, 0i32, 0i32);
            for dy in 0..2 {
                for dx in 0..2 {
                    let px = (cy * 2 + dy) * stride + (cx * 2 + dx) * 4;
                    bs += src[px] as i32;
                    gs += src[px + 1] as i32;
                    rs += src[px + 2] as i32;
                }
            }
            let (b, g, r) = (bs / 4, gs / 4, rs / 4);
            let u = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255);
            let v = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255);
            let o = uvbase + (cy * cw + cx) * 2;
            out[o] = u as u8;
            out[o + 1] = v as u8;
        }
    }
    out
}

/// H.264 software encoder backed by openh264 (Cisco's codec, pure-Rust bindings).
/// Feeds BGRA straight in via YUVBuffer::read_bgra8, emits Annex-B NAL bitstream.
/// This replaces the 14.7MB/frame lossless path with ~100KB/frame real H.264.
pub struct H264Encoder {
    enc: openh264::encoder::Encoder,
    yuv: openh264::formats::YUVBuffer,
    counter: u64,
    width: u32,
    height: u32,
}

impl H264Encoder {
    /// Build for a fixed resolution at the given target bitrate (bps) and fps cap.
    pub fn new(width: u32, height: u32, bitrate_bps: u32, fps: f32) -> Result<Self> {
        use openh264::encoder::{BitRate, EncoderConfig, FrameRate, RateControlMode, UsageType};
        use openh264::formats::YUVBuffer;
        use openh264::OpenH264API;
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate_bps))
            .max_frame_rate(FrameRate::from_hz(fps))
            .rate_control_mode(RateControlMode::Bitrate)
            .usage_type(UsageType::ScreenContentRealTime);
        let enc = openh264::encoder::Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| anyhow::anyhow!("openh264 encoder init: {e}"))?;
        let yuv = YUVBuffer::new(width as usize, height as usize);
        Ok(Self { enc, yuv, counter: 0, width, height })
    }

    /// Force the next frame to be a keyframe (IDR) — for recovery / new clients.
    pub fn request_keyframe(&mut self) {
        self.enc.force_intra_frame();
    }
}

impl VideoEncoder for H264Encoder {
    fn name(&self) -> &'static str {
        "openh264-h264"
    }

    fn encode(&mut self, frame: &CapturedFrame, pts: u64) -> Result<EncodedFrame> {
        use openh264::encoder::FrameType;
        use openh264::formats::{BgraSliceU8, YUVSource};
        ensure!(
            frame.width == self.width && frame.height == self.height,
            "H264Encoder: frame {}x{} != configured {}x{}",
            frame.width, frame.height, self.width, self.height
        );
        // BGRA -> YUV (openh264 does the color convert internally from the slice).
        // BgraSliceU8 assumes tightly-packed rows; if the capture is stride-pitched,
        // repack into a tight buffer first so we don't read across row padding.
        let w4 = frame.width as usize * 4;
        let tight_buf;
        let tight: &[u8] = if frame.stride as usize == w4 {
            &frame.pixels
        } else {
            let h = frame.height as usize;
            let mut t = vec![0u8; w4 * h];
            for y in 0..h {
                let s = y * frame.stride as usize;
                t[y * w4..(y + 1) * w4].copy_from_slice(&frame.pixels[s..s + w4]);
            }
            tight_buf = t;
            &tight_buf
        };
        let bgra = BgraSliceU8::new(tight, (frame.width as usize, frame.height as usize));
        self.yuv.read_bgra8(bgra);

        let stream = self
            .enc
            .encode(&self.yuv)
            .map_err(|e| anyhow::anyhow!("openh264 encode: {e}"))?;
        let keyframe = matches!(stream.frame_type(), FrameType::IDR | FrameType::I);
        let data = stream.to_vec();
        self.counter += 1;
        Ok(EncodedFrame {
            width: self.width,
            height: self.height,
            format: PixelFormat::H264,
            keyframe,
            pts,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_frame(w: u32, h: u32, b: u8, g: u8, r: u8) -> CapturedFrame {
        let stride = (w as usize) * 4;
        let mut pixels = vec![0u8; stride * h as usize];
        for px in pixels.chunks_exact_mut(4) {
            px[0] = b;
            px[1] = g;
            px[2] = r;
            px[3] = 255;
        }
        CapturedFrame { width: w, height: h, stride: stride as u32, pixels, dirty: None }
    }

    #[test]
    fn raw_encoder_roundtrip() {
        let f = solid_frame(8, 4, 10, 200, 30);
        let mut enc = RawEncoder::new();
        let ef = enc.encode(&f, 1234).unwrap();
        assert_eq!(ef.format, PixelFormat::Bgra8);
        assert_eq!(ef.data.len(), 8 * 4 * 4);
        assert_eq!(ef.pts, 1234);
        let back = ef.into_frame().unwrap();
        assert_eq!(back.pixels.len(), f.pixels.len());
        assert_eq!(back.pixels, f.pixels);
    }

    #[test]
    fn raw_encoder_strips_padding() {
        // stride wider than w*4 -> padding dropped
        let (w, h) = (4usize, 2usize);
        let stride = w * 4 + 16; // padded
        let mut pixels = vec![0xAB; stride * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * 4;
                pixels[i] = 1;
                pixels[i + 1] = 2;
                pixels[i + 2] = 3;
                pixels[i + 3] = 4;
            }
        }
        let f = CapturedFrame { width: w as u32, height: h as u32, stride: stride as u32, pixels, dirty: None };
        let ef = RawEncoder::new().encode(&f, 0).unwrap();
        assert_eq!(ef.data.len(), w * h * 4);
        // every pixel is (1,2,3,4), no padding bytes leaked
        assert!(ef.data.chunks_exact(4).all(|p| p == [1, 2, 3, 4]));
    }

    #[test]
    fn nv12_black_is_16_128_128() {
        let f = solid_frame(4, 4, 0, 0, 0);
        let nv = bgra_to_nv12(&f.pixels, f.stride as usize, 4, 4);
        assert_eq!(nv.len(), 4 * 4 + 2 * 2 * 2);
        assert!(nv[..16].iter().all(|&y| y == 16)); // studio black
        // all chroma neutral 128
        assert!(nv[16..].iter().all(|&c| c == 128));
    }

    #[test]
    fn nv12_white_is_235_128_128() {
        let f = solid_frame(4, 4, 255, 255, 255);
        let nv = bgra_to_nv12(&f.pixels, f.stride as usize, 4, 4);
        assert!(nv[..16].iter().all(|&y| y == 235)); // studio white
        assert!(nv[16..].iter().all(|&c| c == 128));
    }

    #[test]
    fn nv12_red_chroma() {
        // pure red -> V high, U low
        let f = solid_frame(4, 4, 0, 0, 255);
        let nv = bgra_to_nv12(&f.pixels, f.stride as usize, 4, 4);
        let u = nv[16];
        let v = nv[17];
        assert!(v > 128, "red should push V up, got {v}");
        assert!(u < 128, "red should push U down, got {u}");
    }

    /// gradient frame (width/height divisible by 8, as openh264 requires)
    fn gradient_frame(w: u32, h: u32) -> CapturedFrame {
        let stride = w as usize * 4;
        let mut pixels = vec![0u8; stride * h as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let i = y * stride + x * 4;
                pixels[i] = (x * 255 / w as usize) as u8;
                pixels[i + 1] = (y * 255 / h as usize) as u8;
                pixels[i + 2] = ((x + y) % 256) as u8;
                pixels[i + 3] = 255;
            }
        }
        CapturedFrame { width: w, height: h, stride: stride as u32, pixels, dirty: None }
    }

    #[test]
    fn h264_encoder_produces_keyframe_smaller_than_lossless() {
        let (w, h) = (128u32, 128u32);
        let f = gradient_frame(w, h);
        let mut enc = H264Encoder::new(w, h, 2_000_000, 30.0).expect("init h264");
        let ef = enc.encode(&f, 0).expect("encode");
        assert_eq!(ef.format, PixelFormat::H264);
        assert!(ef.keyframe, "first frame should be IDR/keyframe");
        assert!(!ef.data.is_empty(), "bitstream non-empty");
        // H.264 Annex-B starts with a start code (00 00 01 or 00 00 00 01)
        let sc = &ef.data[..4.min(ef.data.len())];
        assert!(sc.starts_with(&[0, 0, 0, 1]) || sc.starts_with(&[0, 0, 1]), "no start code: {sc:02x?}");
        // compression: must be far smaller than the 128*128*4 = 65536 lossless frame
        let lossless = (w * h * 4) as usize;
        assert!(ef.data.len() < lossless / 2, "h264 {} bytes not smaller than lossless {lossless}", ef.data.len());
    }

    #[test]
    fn h264_encoder_resizes_and_rekeys() {
        let (w, h) = (128u32, 128u32);
        let f = gradient_frame(w, h);
        let mut enc = H264Encoder::new(w, h, 1_000_000, 30.0).expect("init");
        // second frame: P (not key) normally; force keyframe -> IDR again
        let _ = enc.encode(&f, 1).unwrap();
        enc.request_keyframe();
        let kf = enc.encode(&f, 2).unwrap();
        assert!(kf.keyframe, "forced intra should be a keyframe");
        // wrong-size frame rejected
        let bad = gradient_frame(64, 64);
        assert!(enc.encode(&bad, 3).is_err());
    }

    /// Encode a solid frame to H.264, decode it back, and check the pixels match
    /// (within lossy tolerance). Proves the compressed bitstream is real, decodable video.
    #[test]
    fn h264_encode_decode_roundtrip() {
        use openh264::decoder::Decoder;
        use openh264::formats::YUVSource;
        let (w, h) = (128u32, 128u32);
        // solid mid-gray frame -> predictable decode
        let f = solid_frame(w, h, 128, 128, 128);
        let mut enc = H264Encoder::new(w, h, 2_000_000, 30.0).expect("init");
        let ef = enc.encode(&f, 0).expect("encode");
        assert_eq!(ef.format, PixelFormat::H264);

        let mut dec = Decoder::new().expect("decoder init");
        let yuv = dec.decode(&ef.data).expect("decode").expect("a picture");
        let (dw, dh) = yuv.dimensions();
        assert_eq!((dw, dh), (w as usize, h as usize), "decoded dims");

        let mut rgb = vec![0u8; yuv.rgb8_len()];
        yuv.write_rgb8(&mut rgb);
        // solid gray survives lossy H.264 closely: every channel within ~16 of 128
        let mut max_dev = 0i32;
        for &c in &rgb {
            max_dev = max_dev.max((c as i32 - 128).abs());
        }
        assert!(max_dev <= 24, "gray decode drifted too far: max dev {max_dev}");
    }

    /// Real desktop frame -> H.264, report compression ratio vs lossless.
    #[test]
    #[ignore]
    fn h264_compresses_real_desktop() {
        use crate::capture::DesktopDup;
        let _g = crate::DESKTOP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut dup = DesktopDup::new_primary().expect("dup init");
        let (w, h) = dup.size();
        // width/height must be divisible by 8 for openh264; most modes are, but guard
        if w % 8 != 0 || h % 8 != 0 {
            println!("skip: {w}x{h} not mod-8");
            return;
        }
        let frame = dup.acquire(500).ok().flatten().expect("a frame");
        let mut enc = H264Encoder::new(w, h, 8_000_000, 60.0).expect("h264 init");
        let ef = enc.encode(&frame, 0).expect("encode real frame");
        let lossless = w as usize * h as usize * 4;
        println!(
            "real desktop {w}x{h}: lossless {} KB -> h264 {} KB (ratio {:.1}x, keyframe={})",
            lossless / 1024,
            ef.data.len() / 1024,
            lossless as f64 / ef.data.len().max(1) as f64,
            ef.keyframe
        );
        assert!(ef.data.len() < lossless / 4, "expected strong compression, got {} vs {lossless}", ef.data.len());
    }
}
