//! LeeConnect HOST: streams captured video + audio + cursor + stats,
//! injects received input + gamepad, and serves control RPCs (multi-screen,
//! quality, privacy screen, follow-mode resolution, remote apps, system actions).
//! Run: cargo run --bin host

use anyhow::{Context, Result};
use leeconnect::audio::{AudioPacket, WasapiCapture, AUDIO_CHANNELS, AUDIO_SAMPLE_RATE};
use leeconnect::capabilities::Capabilities;
use leeconnect::capture::{enumerate_monitors, monitor_count, CapturedFrame, DesktopDup};
use leeconnect::display::DisplayRestoreGuard;
use leeconnect::encoder::{create_best_encoder, has_mf_h264_hw, QualityProfile, VideoEncoder};
use leeconnect::frame::{Frame, FrameType};
use leeconnect::gamepad::{is_vigem_available, VigemInjector};
use leeconnect::input::{decode_input, Injector, InputEvent, Screen};
use leeconnect::msg::{self, ControlMsg};
use leeconnect::transport;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

const PORT: u16 = 44330;

#[derive(Clone)]
struct VideoReconfig {
    monitor_idx: Option<usize>,
    profile: Option<QualityProfile>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut monitor_idx: usize = 0;
    let mut max_frames: u32 = u32::MAX;
    let argv: Vec<String> = std::env::args().collect();
    if let Some(pos) = argv.iter().position(|a| a == "--monitor") {
        if let Some(v) = argv.get(pos + 1).and_then(|v| v.parse::<usize>().ok()) {
            monitor_idx = v;
        } else {
            eprintln!("[host] --monitor needs a number (see --list-monitors)");
            std::process::exit(2);
        }
    }
    if let Some(pos) = argv.iter().position(|a| a == "--max-frames") {
        if let Some(v) = argv.get(pos + 1).and_then(|v| v.parse::<u32>().ok()) {
            max_frames = v;
        }
    }
    if argv.iter().any(|a| a == "--list-monitors") {
        match enumerate_monitors() {
            Ok(list) => {
                println!("[host] {} output(s) attached", list.len());
                for m in list {
                    let tag = if m.is_primary { " [primary]" } else { "" };
                    println!("  #{}: {}x{} ({}){tag}", m.index, m.width, m.height, m.name);
                }
            }
            Err(e) => eprintln!("[host] enumerate failed: {e}"),
        }
        return Ok(());
    }

    // --pair [signal_host:port]: register with the signaling server, print a pairing code
    let pair_mode = argv.iter().any(|a| a == "--pair");
    let signal: String = argv
        .iter()
        .position(|a| a == "--pair")
        .and_then(|p| argv.get(p + 1))
        .cloned()
        .unwrap_or_else(|| leeconnect::pair::DEFAULT_SIGNAL.to_owned());

    let lan_ip = leeconnect::pair::local_ip().map(|i| i.to_string());
    let extra_sans: Vec<String> = lan_ip.iter().cloned().collect();
    let (sc, ca_der) = transport::gen_server_config_persisted_extra(&extra_sans)?;
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

    // Auxiliary bidi stream dispatcher: handles file upload/download and TCP-over-QUIC portmap
    let conn_aux = conn.clone();
    tokio::spawn(async move {
        while let Ok((mut s, mut r)) = conn_aux.accept_bi().await {
            tokio::spawn(async move {
                let Ok(first) = transport::read_frame(&mut r).await else { return };
                match first.ftype {
                    FrameType::File => {
                        if let Ok(op) = leeconnect::file::FileOp::from_payload(&first.payload) {
                            match op {
                                leeconnect::file::FileOp::Req { path, .. } => {
                                    let _ = leeconnect::file::send_file(std::path::Path::new(&path), &mut s).await;
                                }
                                leeconnect::file::FileOp::Meta { .. } => {
                                    let out_dir = std::path::Path::new("target/received");
                                    if let Ok(saved) = leeconnect::file::recv_file_with_first(first, &mut r, out_dir).await {
                                        println!("[host] received file -> {}", saved.display());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    FrameType::Control => {
                        let txt = String::from_utf8_lossy(&first.payload);
                        if let Some(port_str) = txt.strip_prefix("portmap_connect:") {
                            if let Ok(port) = port_str.trim().parse::<u16>() {
                                let target = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
                                if let Ok(mut tcp) = tokio::net::TcpStream::connect(target).await {
                                    let (mut tr, mut tw) = tcp.split();
                                    let mut buf1 = vec![0u8; 32 * 1024];
                                    let mut buf2 = vec![0u8; 32 * 1024];
                                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                    let _ = tokio::join!(
                                        async {
                                            loop {
                                                match tr.read(&mut buf1).await {
                                                    Ok(0) | Err(_) => { let _ = s.finish(); break; }
                                                    Ok(n) => { if s.write_all(&buf1[..n]).await.is_err() { break; } }
                                                }
                                            }
                                        },
                                        async {
                                            loop {
                                                match r.read(&mut buf2).await {
                                                    Ok(None) | Ok(Some(0)) | Err(_) => break,
                                                    Ok(Some(n)) => { if tw.write_all(&buf2[..n]).await.is_err() { break; } }
                                                }
                                            }
                                        }
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            });
        }
    });

    let mut caps = Capabilities::FPS_144
        | Capabilities::QUALITY_NORMAL
        | Capabilities::SCREEN_SUPER
        | Capabilities::DEVICE_MOUSE
        | Capabilities::DEVICE_KEYBOARD
        | Capabilities::SIMULATE_MOUSE
        | Capabilities::AUDIO
        | Capabilities::CLIPBOARD
        | Capabilities::FILE_TRANS
        | Capabilities::PRIVACY_SCREEN
        | Capabilities::SYSTEM_ACTIONS
        | Capabilities::REMOTE_APPS
        | Capabilities::FOLLOW_MODE
        | Capabilities::ANNOTATION;
    if monitor_count().unwrap_or(1) > 1 {
        caps |= Capabilities::MULTI_SCREEN;
    }
    if is_vigem_available() {
        caps |= Capabilities::DEVICE_GAMEPAD;
    }
    if has_mf_h264_hw() {
        caps |= Capabilities::SCREEN_CHROMA;
    }

    let hello = msg::encode(&ControlMsg::CapabilityExchange { caps: caps.to_u64() });
    transport::write_frame(&mut send, &Frame::new(FrameType::Control, 0, 0, hello.into_bytes())).await?;

    if let Ok(monitors) = enumerate_monitors() {
        let ml = msg::encode(&ControlMsg::MonitorList { monitors, active: monitor_idx });
        transport::write_frame(&mut send, &Frame::new(FrameType::Control, 0, 1, ml.into_bytes())).await?;
    }

    let dup = DesktopDup::new_for_output(monitor_idx)?;
    let (w, h) = dup.size();
    println!("[host] desktop #{monitor_idx} {w}x{h}");

    let screen_w = Arc::new(AtomicU32::new(w));
    let screen_h = Arc::new(AtomicU32::new(h));

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(16);
    let stop = Arc::new(AtomicBool::new(false));
    let audio_enabled = Arc::new(AtomicBool::new(true));
    let sent_frames = Arc::new(AtomicU32::new(0));
    let injected = Arc::new(AtomicU64::new(0));
    let video_reconfig: Arc<Mutex<Option<VideoReconfig>>> = Arc::new(Mutex::new(None));

    // Video + cursor + stats pump
    let stop_v = stop.clone();
    let cnt_v = sent_frames.clone();
    let reconf_v = video_reconfig.clone();
    let sw_v = screen_w.clone();
    let sh_v = screen_h.clone();
    let tx_v = tx.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut dup = dup;
        let mut cur_mon = monitor_idx;
        let mut cur_profile = QualityProfile::from_preset(1);
        let (mut cw, mut ch) = dup.size();
        let rt = tokio::runtime::Handle::current();
        let mut enc: Box<dyn VideoEncoder> = create_best_encoder(cw, ch, cur_profile, false)?;
        println!("[host] encoder: {}", enc.name());
        let mut last: Option<CapturedFrame> = None;
        let mut seq = 0u32;

        while !stop_v.load(Ordering::Relaxed) && seq < max_frames {
            // Check pending monitor switch / quality reconfig
            if let Some(rc) = reconf_v.lock().unwrap().take() {
                let target_mon = rc.monitor_idx.unwrap_or(cur_mon);
                if let Some(prof) = rc.profile {
                    cur_profile = prof;
                }
                if target_mon != cur_mon {
                    // Release old duplication before acquiring new monitor
                    drop(dup);
                    dup = DesktopDup::new_for_output(target_mon)?;
                    cur_mon = target_mon;
                    last = None;
                }
                let (nw, nh) = dup.size();
                cw = nw;
                ch = nh;
                sw_v.store(cw, Ordering::Relaxed);
                sh_v.store(ch, Ordering::Relaxed);
                enc = create_best_encoder(cw, ch, cur_profile, false)?;
                println!("[host] reconfigured desktop #{cur_mon} {cw}x{ch} enc={}", enc.name());
            }

            let t_cap = std::time::Instant::now();
            let frame = match dup.acquire(33) {
                Ok(Some(f)) => {
                    last = Some(f.clone());
                    f
                }
                Ok(None) => last.clone().unwrap_or_else(|| synthetic_frame(cw, ch)),
                Err(_) => {
                    // Resolution changed (e.g. follow mode): recreate DesktopDup
                    if let Ok(nd) = DesktopDup::new_for_output(cur_mon) {
                        dup = nd;
                        let (nw, nh) = dup.size();
                        if nw != cw || nh != ch {
                            cw = nw;
                            ch = nh;
                            sw_v.store(cw, Ordering::Relaxed);
                            sh_v.store(ch, Ordering::Relaxed);
                            if let Ok(ne) = create_best_encoder(cw, ch, cur_profile, false) {
                                enc = ne;
                            }
                            last = None;
                        }
                    }
                    last.clone().unwrap_or_else(|| synthetic_frame(cw, ch))
                }
            };
            let capture_us = t_cap.elapsed().as_micros().min(u32::MAX as u128) as u32;
            let t_enc = std::time::Instant::now();
            let ef = enc.encode(&frame, seq as u64)?;
            let encode_us = t_enc.elapsed().as_micros().min(u32::MAX as u128) as u32;

            let bytes = Frame::new(FrameType::Video, 0, seq, ef.to_wire()).encode();
            if rt.block_on(tx_v.send(bytes)).is_err() {
                break;
            }
            let cur = leeconnect::cursor::poll();
            let cbytes = Frame::new(FrameType::Cursor, 0, seq, cur.encode()).encode();
            if rt.block_on(tx_v.send(cbytes)).is_err() {
                break;
            }
            let st = leeconnect::stats::Stats { capture_us, encode_us };
            let sbytes = Frame::new(FrameType::Stats, 0, seq, st.encode()).encode();
            if rt.block_on(tx_v.send(sbytes)).is_err() {
                break;
            }
            cnt_v.fetch_add(1, Ordering::Relaxed);
            seq += 1;
        }
        Ok(())
    });

    // Audio loopback capture pump (non-fatal if headless / no sound device)
    let stop_a = stop.clone();
    let audio_en_a = audio_enabled.clone();
    let tx_a = tx.clone();
    let rt_handle = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let Ok(mut cap) = WasapiCapture::new_loopback() else {
            eprintln!("[host] WASAPI loopback unavailable; audio disabled");
            return;
        };
        let mut aseq = 0u32;
        while !stop_a.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(20));
            if let Ok(pcm) = cap.read_pcm() {
                if pcm.is_empty() || !audio_en_a.load(Ordering::Relaxed) {
                    continue;
                }
                if let Ok(pkt) = AudioPacket::encode_adpcm(AUDIO_SAMPLE_RATE, AUDIO_CHANNELS, &pcm) {
                    let wire = pkt.into_frame(aseq).encode();
                    aseq = aseq.wrapping_add(1);
                    if rt_handle.block_on(tx_a.send(wire)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Writer task
    let tx_ctl = tx.clone();
    let writer = tokio::spawn(async move {
        while let Some(bytes) = rx.recv().await {
            if send.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = send.finish();
    });

    // Reader loop: input + gamepad + control RPCs
    let inj = injected.clone();
    let mut vigem: Option<VigemInjector> = None;
    let mut vigem_tried = false;
    let mut display_guard: Option<DisplayRestoreGuard> = None;

    loop {
        let f = match transport::read_frame(&mut recv).await {
            Ok(f) => f,
            Err(_) => break,
        };
        match f.ftype {
            FrameType::Input => {
                if let Ok(ev) = decode_input(&f.payload) {
                    match ev {
                        InputEvent::Gamepad(gp) => {
                            if !vigem_tried {
                                vigem_tried = true;
                                vigem = VigemInjector::open().ok();
                            }
                            if let Some(v) = vigem.as_mut() {
                                if v.update(gp).is_ok() {
                                    inj.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        other => {
                            let injector = Injector::new(Screen {
                                width: screen_w.load(Ordering::Relaxed),
                                height: screen_h.load(Ordering::Relaxed),
                            });
                            if injector.inject(other).is_ok() {
                                inj.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
            FrameType::Control => {
                let txt = String::from_utf8_lossy(&f.payload).to_string();
                if txt.contains("\"end\"") {
                    break;
                }
                match msg::decode(&txt) {
                    Ok(ControlMsg::SystemAction { action }) => {
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
                        let reply = msg::encode(&ControlMsg::SystemActionResult { action, ok, detail });
                        let _ = tx_ctl
                            .send(Frame::new(FrameType::Control, 0, 0, reply.into_bytes()).encode())
                            .await;
                    }
                    Ok(ControlMsg::SwitchMonitor { index }) => {
                        *video_reconfig.lock().unwrap() = Some(VideoReconfig {
                            monitor_idx: Some(index),
                            profile: None,
                        });
                        if let Ok(monitors) = enumerate_monitors() {
                            let reply = msg::encode(&ControlMsg::MonitorList { monitors, active: index });
                            let _ = tx_ctl
                                .send(Frame::new(FrameType::Control, 0, 0, reply.into_bytes()).encode())
                                .await;
                        }
                    }
                    Ok(ControlMsg::QualityConfig { bitrate_bps, fps, chroma_444, hdr }) => {
                        let prof = QualityProfile {
                            bitrate_bps,
                            fps,
                            chroma: if chroma_444 {
                                leeconnect::encoder::ChromaSampling::Yuv444
                            } else {
                                leeconnect::encoder::ChromaSampling::Yuv420
                            },
                            hdr,
                        };
                        *video_reconfig.lock().unwrap() = Some(VideoReconfig {
                            monitor_idx: None,
                            profile: Some(prof),
                        });
                    }
                    Ok(ControlMsg::AudioToggle { enabled }) => {
                        audio_enabled.store(enabled, Ordering::Relaxed);
                    }
                    Ok(ControlMsg::SetResolution { width, height }) => {
                        if display_guard.is_none() {
                            display_guard = Some(DisplayRestoreGuard::capture());
                        }
                        let (ok, detail) = match leeconnect::display::apply_mode(width, height) {
                            Ok(m) => (true, format!("已切换为 {}x{}", m.width, m.height)),
                            Err(e) => (false, e.to_string()),
                        };
                        let reply = msg::encode(&ControlMsg::SystemActionResult {
                            action: "follow_res".into(),
                            ok,
                            detail,
                        });
                        let _ = tx_ctl
                            .send(Frame::new(FrameType::Control, 0, 0, reply.into_bytes()).encode())
                            .await;
                    }
                    Ok(ControlMsg::RestoreResolution) => {
                        if let Some(mut g) = display_guard.take() {
                            g.restore();
                        }
                    }
                    Ok(ControlMsg::AppListRequest) => {
                        let apps = leeconnect::apps::enumerate_apps(48);
                        let reply = msg::encode(&ControlMsg::AppList { apps });
                        let _ = tx_ctl
                            .send(Frame::new(FrameType::Control, 0, 0, reply.into_bytes()).encode())
                            .await;
                    }
                    Ok(ControlMsg::LaunchApp { path }) => {
                        let (ok, detail) = match leeconnect::apps::launch_app(&path) {
                            Ok(m) => (true, m),
                            Err(e) => (false, e.to_string()),
                        };
                        let reply = msg::encode(&ControlMsg::LaunchAppResult {
                            name: path,
                            ok,
                            detail,
                        });
                        let _ = tx_ctl
                            .send(Frame::new(FrameType::Control, 0, 0, reply.into_bytes()).encode())
                            .await;
                    }
                    _ => {}
                }
            }
            FrameType::Clipboard => {
                if let Ok(ev) = leeconnect::clipboard::ClipboardEvent::from_payload(&f.payload) {
                    let _ = leeconnect::clipboard::apply(&ev);
                }
            }
            _ => {}
        }
    }

    stop.store(true, Ordering::Relaxed);
    drop(display_guard);
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
