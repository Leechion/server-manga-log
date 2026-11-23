//! H.264 software decoder backed by openh264. The controller side of the video
//! pipeline: takes EncodedFrame{format:H264} Annex-B bitstreams and reconstructs
//! BGRA pixels for rendering. Pairs with `encoder::H264Encoder`.

use crate::encoder::{EncodedFrame, PixelFormat};
use anyhow::{ensure, Context, Result};

pub struct H264Decoder {
    dec: openh264::decoder::Decoder,
}

impl H264Decoder {
    pub fn new() -> Result<Self> {
        let dec = openh264::decoder::Decoder::new().map_err(|e| anyhow::anyhow!("openh264 decoder init: {e}"))?;
        Ok(Self { dec })
    }

    /// Decode one H.264 bitstream frame into BGRA pixels.
    /// Returns None if the decoder needs more input before producing a picture.
    pub fn decode(&mut self, ef: &EncodedFrame) -> Result<Option<DecodedPicture>> {
        ensure!(ef.format == PixelFormat::H264, "H264Decoder: expected H264, got {:?}", ef.format);
        use openh264::formats::YUVSource;
        let yuv = self
            .dec
            .decode(&ef.data)
            .map_err(|e| anyhow::anyhow!("openh264 decode: {e}"))?;
        let Some(yuv) = yuv else { return Ok(None) };
        let (w, h) = yuv.dimensions();
        let mut rgb = vec![0u8; yuv.rgb8_len()];
        yuv.write_rgb8(&mut rgb);
        // openh264 emits RGB (R,G,B) tightly packed; expand to BGRA for the renderer.
        let mut bgra = Vec::with_capacity(w * h * 4);
        for px in rgb.chunks_exact(3) {
            bgra.extend_from_slice(&[px[2], px[1], px[0], 255]); // R,G,B -> B,G,R,A
        }
        Ok(Some(DecodedPicture { width: w as u32, height: h as u32, pixels: bgra, pts: ef.pts }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPicture {
    pub width: u32,
    pub height: u32,
    /// BGRA8, tightly packed (stride == width*4)
    pub pixels: Vec<u8>,
    pub pts: u64,
}

impl DecodedPicture {
    /// Mean luma (diagnostic: is this a real picture or black?).
    pub fn mean_luma(&self) -> f64 {
        if self.pixels.is_empty() {
            return 0.0;
        }
        let mut sum = 0u64;
        for px in self.pixels.chunks_exact(4) {
            sum += (px[0] as u64 + px[1] as u64 + px[2] as u64) / 3;
        }
        sum as f64 / (self.pixels.len() / 4) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::{H264Encoder, VideoEncoder};

    fn gradient_frame(w: u32, h: u32) -> crate::capture::CapturedFrame {
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
        crate::capture::CapturedFrame { width: w, height: h, stride: stride as u32, pixels, dirty: None }
    }

    #[test]
    fn encode_then_decode_reconstructs_frame() {
        let (w, h) = (128u32, 128u32);
        let f = gradient_frame(w, h);
        let mut enc = H264Encoder::new(w, h, 4_000_000, 30.0).expect("enc init");
        let ef = enc.encode(&f, 7).expect("encode");

        let mut dec = H264Decoder::new().expect("dec init");
        let pic = dec.decode(&ef).expect("decode").expect("a picture");
        assert_eq!((pic.width, pic.height), (w, h));
        assert_eq!(pic.pixels.len(), w as usize * h as usize * 4);
        assert_eq!(pic.pts, 7);
        // gradient has content -> mean luma should be mid-range, not black
        let l = pic.mean_luma();
        assert!(l > 20.0 && l < 235.0, "decoded gradient luma {l} out of range");
    }

    #[test]
    fn decode_rejects_non_h264() {
        let ef = EncodedFrame {
            width: 4,
            height: 4,
            format: PixelFormat::Bgra8,
            keyframe: true,
            pts: 0,
            data: vec![0u8; 64],
        };
        let mut dec = H264Decoder::new().unwrap();
        assert!(dec.decode(&ef).is_err());
    }
}
