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
    // --monitor <idx> selects the output to duplicate (default 0 = primary).
    let mut monitor_idx: usize = 0;
    let argv: Vec<String> = std::env::args().collect();
    if let Some(pos) = argv.iter().position(|a| a == "--monitor") {
        if let Some(v) = argv.get(pos + 1).and_then(|v| v.parse::<usize>().ok()) {
            monitor_idx = v;
        } else {
            eprintln!("[host] --monitor needs a number (see --list-monitors)");
            std::process::exit(2);
        }
    }
    if argv.iter().any(|a| a == "--list-monitors") {
        match leeconnect::capture::monitor_count() {
            Ok(n) => {
                println!("[host] {n} output(s) attached");
                for i in 0..n {
                    match DesktopDup::new_for_output(i) {
                        Ok(d) => {
                            let (w, h) = d.size();
                            println!("  #{i}: {w}x{h}");
                        }
                        Err(e) => println!("  #{i}: unavailable ({e})"),
                    }
                }
            }
            Err(e) => eprintln!("[host] enumerate failed: {e}"),
        }
        return Ok(());
    }

    // --pair [signal_host:port]: register with the signaling server, print a
    // pairing code, and wait for a controller to connect (LAN discovery, #3).
    let pair_mode = argv.iter().any(|a| a == "--pair");
    let signal: String = argv
        .iter()
        .position(|a| a == "--pair")
        .and_then(|p| argv.get(p + 1))
        .cloned()
        .unwrap_or_else(|| leeconnect::pair::DEFAULT_SIGNAL.to_owned());

    // LAN IP goes into the leaf SAN so the ctrl can name-verify when connecting by IP.
    let lan_ip = leeconnect::pair::local_ip().map(|i| i.to_string());
    let extra_sans: Vec<String> = lan_ip.iter().cloned().collect();
    let (sc, ca_der) = transport::gen_server_config_persisted_extra(&extra_sans)?; // CA written to target/ for the controller
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), PORT);
    let server = transport::bind_server(addr, sc)?;
    println!("[host] listening on {addr} (all interfaces)");

    let endpoint_hint = lan_ip
        .map(|ip| format!("{ip}:{PORT}"))
        .unwrap_or_else(|| format!("127.0.0.1:{PORT}"));

    let mut _sig = None;
    if pair_mode {
        let mut s = leeconnect::pair::Signaling::open(&signal)
            .with_context(|| format!("open signaling {signal}"))?;
        let serial = leeconnect::pair::gen_serial(leeconnect::pair::DEFAULT_SECRET, "LEE-HOST-1");
        s.register("LEE-HOST-1", &serial, "lee-host", &endpoint_hint, &ca_der)?;
        let code = s.pair_create()?;
        println!("[host] PAIRING CODE: {code}  (endpoint {endpoint_hint})");
        _sig = Some(s);
    }

    let conn = server.accept().await.context("no incoming")?.await?;
    println!("[host] controller connected");
    let (mut send, mut recv) = conn.accept_bi().await?;

    // capability hello
    let mut caps = Capabilities::FPS_144 | Capabilities::SCREEN_SUPER | Capabilities::DEVICE_MOUSE
        | Capabilities::DEVICE_KEYBOARD | Capabilities::CLIPBOARD | Capabilities::FILE_TRANS;
    let hello = msg::encode(&ControlMsg::CapabilityExchange { caps: caps.to_u64() });
    transport::write_frame(&mut send, &Frame::new(FrameType::Control, 0, 0, hello.into_bytes())).await?;

    let mut dup = DesktopDup::new_for_output(monitor_idx)?;
    if leeconnect::capture::monitor_count().unwrap_or(1) > 1 {
        caps |= Capabilities::MULTI_SCREEN;
    }
    let (w, h) = dup.size();
    println!("[host] desktop #{monitor_idx} {w}x{h}");

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
    let tx_ctl = tx.clone(); // for control replies from the reader loop
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
            let t_cap = std::time::Instant::now();
            let frame = match dup.acquire(50)? {
                Some(f) => {
                    last = Some(f.clone());
                    f
                }
                None => last.clone().unwrap_or_else(|| synthetic_frame(w, h)),
            };
            let capture_us = t_cap.elapsed().as_micros().min(u32::MAX as u128) as u32;
            let t_enc = std::time::Instant::now();
            let ef = enc.encode(&frame, seq as u64)?;
            let encode_us = t_enc.elapsed().as_micros().min(u32::MAX as u128) as u32;
            let bytes = Frame::new(FrameType::Video, 0, seq, ef.to_wire()).encode();
            let tx2 = tx.clone();
            rt.block_on(async move { tx2.send(bytes).await })?;
            // cursor state rides alongside each video frame (9 bytes, negligible)
            let cur = leeconnect::cursor::poll();
            let cbytes = Frame::new(FrameType::Cursor, 0, seq, cur.encode()).encode();
            let tx3 = tx.clone();
            rt.block_on(async move { tx3.send(cbytes).await })?;
            // per-segment latency stats (8 bytes) for the controller's breakdown
            let st = leeconnect::stats::Stats { capture_us, encode_us };
            let sbytes = Frame::new(FrameType::Stats, 0, seq, st.encode()).encode();
            let tx4 = tx.clone();
            rt.block_on(async move { tx4.send(sbytes).await })?;
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
                if let Ok(ControlMsg::SystemAction { action }) = msg::decode(&txt) {
                    println!("[host] system action: {action}");
                    let (ok, detail) = match action.as_str() {
                        "privacy_on" | "privacy_on:confirm" => {
                            match leeconnect::privacy::PrivacyGuard::start() {
                                Ok(_) => (true, "privacy screen engaged".to_string()),
                                Err(e) => (false, e.to_string()),
                            }
                        }
                        "privacy_off" => {
                            leeconnect::privacy::PrivacyGuard::stop();
                            (true, "privacy screen disengaged".to_string())
                        }
                        _ => match leeconnect::system::handle(&action) {
                            Ok(msg) => (true, msg.to_string()),
                            Err(e) => (false, e.to_string()),
                        },
                    };
                    println!("[host]   -> ok={ok} {detail}");
                    let reply = msg::encode(&ControlMsg::SystemActionResult {
                        action,
                        ok,
                        detail,
                    });
                    let _ = tx_ctl
                        .send(Frame::new(FrameType::Control, 0, 0, reply.into_bytes()).encode())
                        .await;
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
