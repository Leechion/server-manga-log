//! LeeConnect HOST: streams captured video + injects received input.
//! One multiplexed QUIC bidi stream (frames tagged by type). Run: cargo run --bin host
//! Listens on 127.0.0.1:44330 (local test); a controller connects to it.

use anyhow::{Context, Result};
use leeconnect::capabilities::Capabilities;
use leeconnect::capture::{CapturedFrame, DesktopDup};
use leeconnect::encoder::{H264Encoder, RawEncoder, VideoEncoder};
use leeconnect::frame::{Frame, FrameType};
use leeconnect::input::{decode_input, Injector, Screen};
use leeconnect::msg::{self, ControlMsg};
use leeconnect::transport;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

const PORT: u16 = 44330;
const MAX_FRAMES: u32 = 600;

#[tokio::main]
async fn main() -> Result<()> {
    let (sc, _ca_der) = transport::gen_server_config_persisted()?; // CA written to target/ for the controller
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), PORT);
    let mut server = transport::bind_server(addr, sc)?;
    println!("[host] listening on {addr}");

    let conn = server.accept().await.context("no incoming")?.await?;
    println!("[host] controller connected");
    let (mut send, mut recv) = conn.accept_bi().await?;

    // capability hello
    let caps = Capabilities::FPS_144 | Capabilities::SCREEN_SUPER | Capabilities::DEVICE_MOUSE
        | Capabilities::DEVICE_KEYBOARD | Capabilities::CLIPBOARD | Capabilities::FILE_TRANS;
    let hello = msg::encode(&ControlMsg::CapabilityExchange { caps: caps.to_u64() });
    transport::write_frame(&mut send, &Frame::new(FrameType::Control, 0, 0, hello.into_bytes())).await?;

    let mut dup = DesktopDup::new_primary()?;
    let (w, h) = dup.size();
    println!("[host] desktop {w}x{h}");

    // pick encoder: H.264 if resolution is mod-8 (openh264 requirement), else lossless raw
    let use_h264 = w % 8 == 0 && h % 8 == 0;
    println!("[host] encoder: {}", if use_h264 { "H.264 (openh264)" } else { "raw-bgra (mod-8 fail)" });

    // video pump: blocking capture+encode -> channel -> writer task owns `send`
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(8);
    let stop = Arc::new(AtomicBool::new(false));
    let sent_frames = Arc::new(AtomicU32::new(0));
    let injected = Arc::new(AtomicU64::new(0));

    let stop_v = stop.clone();
    let cnt_v = sent_frames.clone();
    let use_h264_c = use_h264;
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut dup = dup;
        let rt = tokio::runtime::Handle::current();
        let mut enc: Box<dyn VideoEncoder> = if use_h264_c {
            Box::new(H264Encoder::new(w, h, 8_000_000, 60.0)?)
        } else {
            Box::new(RawEncoder::new())
        };
        let mut last: Option<CapturedFrame> = None;
        let mut seq = 0u32;
        while !stop_v.load(Ordering::Relaxed) && seq < MAX_FRAMES {
            let frame = match dup.acquire(50)? {
                Some(f) => {
                    last = Some(f.clone());
                    f
                }
                None => last.clone().unwrap_or_else(|| synthetic_frame(w, h)),
            };
            let ef = enc.encode(&frame, seq as u64)?;
            let bytes = Frame::new(FrameType::Video, 0, seq, ef.to_wire()).encode();
            let tx2 = tx.clone();
            rt.block_on(async move { tx2.send(bytes).await })?;
            cnt_v.fetch_add(1, Ordering::Relaxed);
            seq += 1;
        }
        Ok(())
    });

    // writer task: drain channel -> send stream
    let writer = tokio::spawn(async move {
        while let Some(bytes) = rx.recv().await {
            if send.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = send.finish();
    });

    // reader loop: inject input, stop on Control(end)
    let injector = Injector::new(Screen { width: w, height: h });
    let inj = injected.clone();
    loop {
        let f = match transport::read_frame(&mut recv).await {
            Ok(f) => f,
            Err(_) => break,
        };
        match f.ftype {
            FrameType::Input => {
                if let Ok(ev) = decode_input(&f.payload) {
                    // zero-delta moves only: verify pipeline without disturbing the cursor
                    if let Ok(()) = injector.inject(ev) {
                        inj.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            FrameType::Control => {
                let txt = String::from_utf8_lossy(&f.payload).to_string();
                if txt.contains("\"end\"") {
                    break;
                }
            }
            _ => {}
        }
    }

    stop.store(true, Ordering::Relaxed);
    let _ = writer.await;
    let n = injected.load(Ordering::Relaxed);
    let sent = sent_frames.load(Ordering::Relaxed);
    conn.close(0u32.into(), b"host-done");
    println!("[host] sent {sent} video frames, injected {n} input events; session closed");
    Ok(())
}

/// Deterministic fallback frame (gradient) when the desktop is static.
fn synthetic_frame(w: u32, h: u32) -> CapturedFrame {
    let stride = w as usize * 4;
    let mut pixels = vec![0u8; stride * h as usize];
    for y in 0..h as usize {
        let v = (y * 255 / h as usize) as u8;
        for x in 0..w as usize {
            let i = y * stride + x * 4;
            pixels[i] = v;
            pixels[i + 1] = (x as u8).wrapping_mul(3);
            pixels[i + 2] = 255 - v;
            pixels[i + 3] = 255;
        }
    }
    CapturedFrame { width: w, height: h, stride: stride as u32, pixels, dirty: None }
}
