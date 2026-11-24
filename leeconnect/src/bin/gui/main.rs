//! LeeConnect GUI client — a UU-remote-style desktop app (egui/eframe).

pub mod app;
pub mod icons;
pub mod shell;
pub mod theme;
pub mod views;
pub mod widgets;

use anyhow::Result;
use app::GuiApp;
use leeconnect::capabilities::Capabilities;
use leeconnect::decoder::H264Decoder;
use leeconnect::encoder::{EncodedFrame, PixelFormat};
use leeconnect::frame::{Frame, FrameType};
use leeconnect::input::{encode_input, InputEvent};
use leeconnect::msg::{self, ControlMsg};
use leeconnect::transport;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_HOST: u16 = 44330;

/// Latest decoded remote frame, shared network-thread -> UI thread.
#[derive(Default)]
pub struct FrameSlot {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    pub seq: u64,
    /// remote cursor state (position in remote pixels), streamed alongside video
    pub cursor: Option<(i32, i32, bool)>,
    /// host-side segment timings from the newest Stats frame (microseconds)
    pub host_capture_us: u32,
    pub host_encode_us: u32,
    /// controller-side decode duration of the newest frame (microseconds)
    pub decode_us: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Handshake,
    Negotiate,
    Stream,
}

impl Stage {
    pub fn name(&self) -> &'static str {
        match self {
            Stage::Handshake => "安全握手 (QUIC + TLS 1.3)",
            Stage::Negotiate => "能力协商与信令交互",
            Stage::Stream => "请求桌面视频流与首帧解码",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Idle,
    Connecting {
        stage: Stage,
        started: Instant,
        done: Vec<(Stage, Duration)>,
    },
    Live,
    Failed(String),
    Closed,
}

pub struct Session {
    pub slot: Arc<Mutex<FrameSlot>>,
    pub status: Arc<Mutex<Status>>,
    pub caps: Arc<Mutex<Option<Capabilities>>>,
    pub host_w: Arc<AtomicU64>,
    pub host_h: Arc<AtomicU64>,
    pub stop: Arc<AtomicBool>,
    pub bytes_rx: Arc<AtomicU64>,
    pub rtt_ms: Arc<AtomicU64>,
    /// pending system-action results echoed by the host (run thread -> UI toast)
    pub sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
}

impl Session {
    /// Spawn the background network thread (own current-thread tokio runtime).
    pub fn spawn(
        host: SocketAddr,
        slot: Arc<Mutex<FrameSlot>>,
        status: Arc<Mutex<Status>>,
        caps: Arc<Mutex<Option<Capabilities>>>,
        hw: Arc<AtomicU64>,
        hh: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        bytes_rx: Arc<AtomicU64>,
        rtt_ms: Arc<AtomicU64>,
        input_rx: Receiver<InputEvent>,
        ctrl_rx: Receiver<ControlMsg>,
        sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
    ) {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    *status.lock().unwrap() = Status::Failed(format!("Runtime init error: {e}"));
                    eprintln!("[gui-session] runtime: {e}");
                    return;
                }
            };
            if let Err(e) = rt.block_on(run(
                host, slot, status.clone(), caps, hw, hh, stop, bytes_rx, rtt_ms, input_rx, ctrl_rx, sys_results,
            )) {
                let err_msg = e.to_string();
                eprintln!("[gui-session] ended: {err_msg}");
                *status.lock().unwrap() = Status::Failed(err_msg);
            }
        });
    }
}

async fn run(
    host: SocketAddr,
    slot: Arc<Mutex<FrameSlot>>,
    status: Arc<Mutex<Status>>,
    caps: Arc<Mutex<Option<Capabilities>>>,
    hw: Arc<AtomicU64>,
    hh: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    bytes_rx: Arc<AtomicU64>,
    rtt_ms: Arc<AtomicU64>,
    input_rx: Receiver<InputEvent>,
    ctrl_rx: Receiver<ControlMsg>,
    sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
) -> Result<()> {
    // 1. Stage: Handshake
    let t0 = Instant::now();
    *status.lock().unwrap() = Status::Connecting {
        stage: Stage::Handshake,
        started: t0,
        done: Vec::new(),
    };

    let ca = match transport::load_ca() {
        Ok(ca) => ca,
        Err(e) => {
            if let Ok(ca) = std::fs::read("leeconnect/target/leeconnect-ca.der") {
                ca
            } else {
                anyhow::bail!("CA 根证书不存在 ({e})，请先启动 host 服务生成证书");
            }
        }
    };
    let client = transport::client_endpoint(&ca)?;
    let conn = match client.connect(host, transport::SNI) {
        Ok(c) => match c.await {
            Ok(conn) => conn,
            Err(e) => {
                anyhow::bail!("握手失败 {host}: {e}");
            }
        },
        Err(e) => {
            anyhow::bail!("连接端点错误 {host}: {e}");
        }
    };

    let handshake_dur = t0.elapsed();
    let mut done_stages = vec![(Stage::Handshake, handshake_dur)];

    // 2. Stage: Negotiate
    let t1 = Instant::now();
    *status.lock().unwrap() = Status::Connecting {
        stage: Stage::Negotiate,
        started: t1,
        done: done_stages.clone(),
    };

    let (mut send, mut recv) = match conn.open_bi().await {
        Ok(pair) => pair,
        Err(e) => anyhow::bail!("打开双向流失败: {e}"),
    };

    let negotiate_dur = t1.elapsed();
    done_stages.push((Stage::Negotiate, negotiate_dur));

    // 3. Stage: Stream
    let t2 = Instant::now();
    *status.lock().unwrap() = Status::Connecting {
        stage: Stage::Stream,
        started: t2,
        done: done_stages,
    };

    let mut decoder = H264Decoder::new()?;
    let mut seq = 0u32;
    let mut tick = tokio::time::interval(Duration::from_millis(16));
    let mut has_received_first_frame = false;
    let stream_start = Instant::now();

    loop {
        // Disconnect check
        if stop.load(Ordering::Relaxed) {
            let _ = conn.close(0u32.into(), b"bye");
            break;
        }

        // Timeout check: 10s waiting for first frame
        if !has_received_first_frame && stream_start.elapsed() > Duration::from_secs(10) {
            anyhow::bail!("等待首帧超时 (10秒未收到画面，请确认 host 正常采集)");
        }

        tokio::select! {
            r = transport::read_frame(&mut recv) => {
                let f = match r { Ok(f) => f, Err(_) => break };
                bytes_rx.fetch_add(f.payload.len() as u64, Ordering::Relaxed);
                rtt_ms.store(conn.rtt().as_millis() as u64, Ordering::Relaxed);

                match f.ftype {
                    FrameType::Video => {
                        let ef = EncodedFrame::from_wire(&f.payload)?;
                        hw.store(ef.width as u64, Ordering::Relaxed);
                        hh.store(ef.height as u64, Ordering::Relaxed);
                        let mut got_picture = false;
                        let t_dec = Instant::now();

                        if ef.format == PixelFormat::H264 {
                            if let Some(pic) = decoder.decode(&ef)? {
                                let mut s = slot.lock().unwrap();
                                s.w = pic.width;
                                s.h = pic.height;
                                bgra_to_rgba_into(&pic.pixels, &mut s.rgba);
                                s.seq += 1;
                                s.decode_us = t_dec.elapsed().as_micros().min(u32::MAX as u128) as u32;
                                got_picture = true;
                            }
                        } else if ef.format == PixelFormat::Bgra8 {
                            let mut s = slot.lock().unwrap();
                            s.w = ef.width;
                            s.h = ef.height;
                            bgra_to_rgba_into(&ef.data, &mut s.rgba);
                            s.seq += 1;
                            s.decode_us = t_dec.elapsed().as_micros().min(u32::MAX as u128) as u32;
                            got_picture = true;
                        }

                        if got_picture && !has_received_first_frame {
                            has_received_first_frame = true;
                            *status.lock().unwrap() = Status::Live;
                        }
                    }
                    FrameType::Cursor => {
                        if let Ok(cur) = leeconnect::cursor::CursorState::decode(&f.payload) {
                            slot.lock().unwrap().cursor = Some((cur.x, cur.y, cur.visible));
                        }
                    }
                    FrameType::Stats => {
                        if let Ok(st) = leeconnect::stats::Stats::decode(&f.payload) {
                            let mut s = slot.lock().unwrap();
                            s.host_capture_us = st.capture_us;
                            s.host_encode_us = st.encode_us;
                        }
                    }
                    FrameType::Control => {
                        let txt = String::from_utf8_lossy(&f.payload).to_string();
                        match msg::decode(&txt) {
                            Ok(ControlMsg::CapabilityExchange { caps: c }) => {
                                *caps.lock().unwrap() = Some(Capabilities::from_u64(c));
                            }
                            Ok(ControlMsg::SystemActionResult { action, ok, detail }) => {
                                sys_results.lock().unwrap().push((action, ok, detail));
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                while let Ok(ev) = input_rx.try_recv() {
                    let fr = Frame::new(FrameType::Input, 1, seq, encode_input(ev));
                    seq = seq.wrapping_add(1);
                    if transport::write_frame(&mut send, &fr).await.is_err() {
                        break;
                    }
                }
                // control-plane messages (system actions, etc.) from the UI thread
                while let Ok(cm) = ctrl_rx.try_recv() {
                    let fr = Frame::new(FrameType::Control, 0, seq, msg::encode(&cm).into_bytes());
                    seq = seq.wrapping_add(1);
                    if transport::write_frame(&mut send, &fr).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    if !stop.load(Ordering::Relaxed) && *status.lock().unwrap() == Status::Live {
        *status.lock().unwrap() = Status::Closed;
    }
    Ok(())
}

/// BGRA (decoder output) -> RGBA (egui texture) into `out`.
pub fn bgra_to_rgba_into(bgra: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(bgra.len());
    for px in bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
}

fn main() -> eframe::Result {
    let mut host_arg: Option<String> = None;
    let mut mode = app::AppMode::Normal;

    for arg in std::env::args().skip(1) {
        if arg == "--gallery" {
            mode = app::AppMode::Gallery;
        } else if arg == "--test-screens" {
            mode = app::AppMode::TestScreens;
        } else if !arg.starts_with("--") {
            host_arg = Some(arg);
        }
    }

    let should_auto_connect = host_arg.is_some();
    let host_str = host_arg.unwrap_or_else(|| format!("127.0.0.1:{DEFAULT_HOST}"));
    let host: SocketAddr = host_str
        .parse()
        .unwrap_or_else(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_HOST));

    let slot = Arc::new(Mutex::new(FrameSlot::default()));
    let status = Arc::new(Mutex::new(if should_auto_connect {
        Status::Connecting {
            stage: Stage::Handshake,
            started: Instant::now(),
            done: Vec::new(),
        }
    } else {
        Status::Idle
    }));
    let caps = Arc::new(Mutex::new(None));
    let hw = Arc::new(AtomicU64::new(0));
    let hh = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let bytes_rx = Arc::new(AtomicU64::new(0));
    let rtt_ms = Arc::new(AtomicU64::new(0));
    let (input_tx, input_rx) = std::sync::mpsc::channel::<InputEvent>();
    let (ctrl_tx, ctrl_rx) = std::sync::mpsc::channel::<ControlMsg>();
    let sys_results: Arc<Mutex<Vec<(String, bool, String)>>> = Arc::new(Mutex::new(Vec::new()));

    if mode == app::AppMode::Normal && should_auto_connect {
        Session::spawn(
            host,
            slot.clone(),
            status.clone(),
            caps.clone(),
            hw.clone(),
            hh.clone(),
            stop.clone(),
            bytes_rx.clone(),
            rtt_ms.clone(),
            input_rx,
            ctrl_rx,
            sys_results.clone(),
        );
    }

    let sess = Session {
        slot,
        status,
        caps,
        host_w: hw,
        host_h: hh,
        stop,
        bytes_rx,
        rtt_ms,
        sys_results,
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("LeeConnect"),
        ..Default::default()
    };
    eframe::run_native(
        "LeeConnect",
        opts,
        Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, sess, input_tx, ctrl_tx, host, mode)) as Box<dyn eframe::App>)),
    )
}
