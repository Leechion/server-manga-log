//! LeeConnect GUI client — a macOS-inspired desktop app (egui/eframe).

pub mod app;
pub mod icons;
pub mod shell;
pub mod theme;
pub mod views;
pub mod widgets;

use anyhow::Result;
use app::GuiApp;
use leeconnect::audio::{AudioPacket, WasapiPlayer};
use leeconnect::capabilities::Capabilities;
use leeconnect::clipboard::ClipboardEvent;
use leeconnect::decoder::H264Decoder;
use leeconnect::encoder::{EncodedFrame, PixelFormat};
use leeconnect::file::FileOp;
use leeconnect::frame::{Frame, FrameType};
use leeconnect::gamepad::poll_xinput;
use leeconnect::input::{encode_input, InputEvent};
use leeconnect::msg::{self, AppEntry, ControlMsg, MonitorDesc};
use leeconnect::transport;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_HOST: u16 = 44330;

#[derive(Clone, Debug)]
pub enum SessionCmd {
    UploadFile(PathBuf),
    DownloadFile { remote_path: String, save_dir: PathBuf },
    StartPortmap { local_port: u16, remote_port: u16 },
    StopPortmap(u16),
    SendClipboard(String),
}

#[derive(Clone, Debug)]
pub struct TransferRecord {
    pub name: String,
    pub direction: &'static str,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct PortmapRule {
    pub local_port: u16,
    pub remote_port: u16,
}

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

pub struct PairedTarget {
    pub sig: leeconnect::pair::Signaling,
    pub outcome: leeconnect::pair::PairOutcome,
    pub stun_addr: Option<SocketAddr>,
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
    /// pending system-action / app-launch / file results echoed to UI toast
    pub sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
    pub monitors: Arc<Mutex<(Vec<MonitorDesc>, usize)>>,
    pub remote_apps: Arc<Mutex<Vec<AppEntry>>>,
    pub file_history: Arc<Mutex<Vec<TransferRecord>>>,
    pub portmap_rules: Arc<Mutex<Vec<PortmapRule>>>,
    pub audio_enabled: Arc<AtomicBool>,
    pub gamepad_enabled: Arc<AtomicBool>,
    pub ca_override: Arc<Mutex<Option<Vec<u8>>>>,
    pub sni_override: Arc<Mutex<Option<String>>>,
}

impl Session {
    /// Spawn the background network thread (own current-thread tokio runtime).
    #[allow(clippy::too_many_arguments)]
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
        cmd_rx: Receiver<SessionCmd>,
        sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
        monitors: Arc<Mutex<(Vec<MonitorDesc>, usize)>>,
        remote_apps: Arc<Mutex<Vec<AppEntry>>>,
        file_history: Arc<Mutex<Vec<TransferRecord>>>,
        portmap_rules: Arc<Mutex<Vec<PortmapRule>>>,
        audio_enabled: Arc<AtomicBool>,
        gamepad_enabled: Arc<AtomicBool>,
        ca_der: Option<Vec<u8>>,
        sni: Option<String>,
        paired_target: Option<PairedTarget>,
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
                host,
                slot,
                status.clone(),
                caps,
                hw,
                hh,
                stop,
                bytes_rx,
                rtt_ms,
                input_rx,
                ctrl_rx,
                cmd_rx,
                sys_results,
                monitors,
                remote_apps,
                file_history,
                portmap_rules,
                audio_enabled,
                gamepad_enabled,
                ca_der,
                sni,
                paired_target,
            )) {
                let err_msg = e.to_string();
                eprintln!("[gui-session] ended: {err_msg}");
                *status.lock().unwrap() = Status::Failed(err_msg);
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
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
    cmd_rx: Receiver<SessionCmd>,
    sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
    monitors: Arc<Mutex<(Vec<MonitorDesc>, usize)>>,
    remote_apps: Arc<Mutex<Vec<AppEntry>>>,
    file_history: Arc<Mutex<Vec<TransferRecord>>>,
    portmap_rules: Arc<Mutex<Vec<PortmapRule>>>,
    audio_enabled: Arc<AtomicBool>,
    gamepad_enabled: Arc<AtomicBool>,
    ca_override: Option<Vec<u8>>,
    sni_override: Option<String>,
    paired_target: Option<PairedTarget>,
) -> Result<()> {
    // 1. Stage: Handshake
    let t0 = Instant::now();
    *status.lock().unwrap() = Status::Connecting {
        stage: Stage::Handshake,
        started: t0,
        done: Vec::new(),
    };

    let (_relay_bridge, conn) = if let Some(mut pt) = paired_target {
        let (conn, bridge, _via) =
            leeconnect::pair::connect_paired(&mut pt.sig, pt.outcome, pt.stun_addr).await?;
        (bridge, conn)
    } else {
        let ca = match ca_override {
            Some(c) if !c.is_empty() => c,
            _ => match transport::load_ca() {
                Ok(ca) => ca,
                Err(e) => {
                    if let Ok(ca) = std::fs::read("leeconnect/target/leeconnect-ca.der") {
                        ca
                    } else {
                        anyhow::bail!("CA 根证书不存在 ({e})，请先启动 host 服务生成证书");
                    }
                }
            },
        };
        let sni = sni_override.unwrap_or_else(|| transport::SNI.to_owned());
        let client = transport::client_endpoint(&ca)?;
        let conn = match client.connect(host, &sni) {
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
        (None, conn)
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

    // Background audio playback thread
    let (audio_tx, audio_rx) = std::sync::mpsc::sync_channel::<AudioPacket>(32);
    let stop_audio = stop.clone();
    let audio_en_play = audio_enabled.clone();
    std::thread::spawn(move || {
        let mut player: Option<WasapiPlayer> = None;
        while !stop_audio.load(Ordering::Relaxed) {
            match audio_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(pkt) => {
                    if !audio_en_play.load(Ordering::Relaxed) {
                        continue;
                    }
                    if player.is_none() {
                        player = WasapiPlayer::new().ok();
                    }
                    if let (Some(pl), Ok(pcm)) = (player.as_mut(), pkt.decode_pcm()) {
                        let _ = pl.write_pcm(&pcm);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let mut decoder = H264Decoder::new()?;
    let mut seq = 0u32;
    let mut tick = tokio::time::interval(Duration::from_millis(16));
    let mut has_received_first_frame = false;
    let stream_start = Instant::now();
    let mut last_gp: Option<leeconnect::gamepad::GamepadState> = None;
    let mut tunnel_tasks: HashMap<u16, tokio::task::JoinHandle<()>> = HashMap::new();

    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = conn.close(0u32.into(), b"bye");
            break;
        }

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

                        if matches!(ef.format, PixelFormat::H264 | PixelFormat::H264_444) {
                            let pic = match decoder.decode(&ef) {
                                Ok(p) => p,
                                Err(_) => {
                                    decoder = H264Decoder::new()?;
                                    decoder.decode(&ef).unwrap_or(None)
                                }
                            };
                            if let Some(pic) = pic {
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
                    FrameType::Audio => {
                        if audio_enabled.load(Ordering::Relaxed) {
                            if let Ok(pkt) = AudioPacket::from_wire(&f.payload) {
                                let _ = audio_tx.try_send(pkt);
                            }
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
                            Ok(ControlMsg::MonitorList { monitors: list, active }) => {
                                *monitors.lock().unwrap() = (list, active);
                            }
                            Ok(ControlMsg::AppList { apps }) => {
                                *remote_apps.lock().unwrap() = apps;
                            }
                            Ok(ControlMsg::LaunchAppResult { name, ok, detail }) => {
                                sys_results.lock().unwrap().push((format!("启动 {name}"), ok, detail));
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
                if gamepad_enabled.load(Ordering::Relaxed) {
                    if let Some(gp) = poll_xinput(0) {
                        if last_gp != Some(gp) {
                            last_gp = Some(gp);
                            let fr = Frame::new(FrameType::Input, 1, seq, encode_input(InputEvent::Gamepad(gp)));
                            seq = seq.wrapping_add(1);
                            let _ = transport::write_frame(&mut send, &fr).await;
                        }
                    }
                }
                while let Ok(cm) = ctrl_rx.try_recv() {
                    let fr = Frame::new(FrameType::Control, 0, seq, msg::encode(&cm).into_bytes());
                    seq = seq.wrapping_add(1);
                    if transport::write_frame(&mut send, &fr).await.is_err() {
                        break;
                    }
                }
                while let Ok(scmd) = cmd_rx.try_recv() {
                    match scmd {
                        SessionCmd::SendClipboard(text) => {
                            let fr = ClipboardEvent::Text(text).into_frame(seq);
                            seq = seq.wrapping_add(1);
                            let _ = transport::write_frame(&mut send, &fr).await;
                        }
                        SessionCmd::UploadFile(path) => {
                            let c = conn.clone();
                            let hist = file_history.clone();
                            let res_q = sys_results.clone();
                            tokio::spawn(async move {
                                let fname = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                let outcome = async {
                                    let (mut s, _) = c.open_bi().await?;
                                    leeconnect::file::send_file(&path, &mut s).await
                                }.await;
                                match outcome {
                                    Ok(bytes) => {
                                        hist.lock().unwrap().insert(0, TransferRecord {
                                            name: fname.clone(),
                                            direction: "上传",
                                            status: format!("已完成 ({:.1} KB)", bytes as f64 / 1024.0),
                                        });
                                        res_q.lock().unwrap().push(("文件上传".into(), true, format!("{fname} ({bytes} B)")));
                                    }
                                    Err(e) => {
                                        hist.lock().unwrap().insert(0, TransferRecord {
                                            name: fname,
                                            direction: "上传",
                                            status: format!("失败: {e}"),
                                        });
                                        res_q.lock().unwrap().push(("文件上传".into(), false, e.to_string()));
                                    }
                                }
                            });
                        }
                        SessionCmd::DownloadFile { remote_path, save_dir } => {
                            let c = conn.clone();
                            let hist = file_history.clone();
                            let res_q = sys_results.clone();
                            tokio::spawn(async move {
                                let outcome = async {
                                    let (mut s, mut r) = c.open_bi().await?;
                                    let req = FileOp::Req { id: 1, path: remote_path.clone() };
                                    transport::write_frame(&mut s, &req.into_frame(0)).await?;
                                    leeconnect::file::recv_file(&mut r, &save_dir).await
                                }.await;
                                match outcome {
                                    Ok(saved) => {
                                        let name = saved.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(remote_path);
                                        hist.lock().unwrap().insert(0, TransferRecord {
                                            name: name.clone(),
                                            direction: "下载",
                                            status: format!("已保存至 {}", saved.display()),
                                        });
                                        res_q.lock().unwrap().push(("文件下载".into(), true, name));
                                    }
                                    Err(e) => {
                                        hist.lock().unwrap().insert(0, TransferRecord {
                                            name: remote_path,
                                            direction: "下载",
                                            status: format!("失败: {e}"),
                                        });
                                        res_q.lock().unwrap().push(("文件下载".into(), false, e.to_string()));
                                    }
                                }
                            });
                        }
                        SessionCmd::StartPortmap { local_port, remote_port } => {
                            if let Some(old) = tunnel_tasks.remove(&local_port) {
                                old.abort();
                            }
                            let c = conn.clone();
                            let res_q = sys_results.clone();
                            let rules = portmap_rules.clone();
                            let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), local_port);
                            match tokio::net::TcpListener::bind(bind_addr).await {
                                Ok(listener) => {
                                    rules.lock().unwrap().push(PortmapRule { local_port, remote_port });
                                    res_q.lock().unwrap().push((
                                        "端口映射".into(),
                                        true,
                                        format!("127.0.0.1:{local_port} → 远端:{remote_port}"),
                                    ));
                                    let h = tokio::spawn(async move {
                                        while let Ok((mut tcp, _)) = listener.accept().await {
                                            let c2 = c.clone();
                                            tokio::spawn(async move {
                                                if let Ok((mut s, mut r)) = c2.open_bi().await {
                                                    let hdr = Frame::new(
                                                        FrameType::Control,
                                                        0,
                                                        0,
                                                        format!("portmap_connect:{remote_port}").into_bytes(),
                                                    );
                                                    if transport::write_frame(&mut s, &hdr).await.is_err() {
                                                        return;
                                                    }
                                                    let (mut tr, mut tw) = tcp.split();
                                                    let mut b1 = vec![0u8; 32 * 1024];
                                                    let mut b2 = vec![0u8; 32 * 1024];
                                                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                                    let _ = tokio::join!(
                                                        async {
                                                            loop {
                                                                match tr.read(&mut b1).await {
                                                                    Ok(0) | Err(_) => { let _ = s.finish(); break; }
                                                                    Ok(n) => { if s.write_all(&b1[..n]).await.is_err() { break; } }
                                                                }
                                                            }
                                                        },
                                                        async {
                                                            loop {
                                                                match r.read(&mut b2).await {
                                                                    Ok(None) | Ok(Some(0)) | Err(_) => break,
                                                                    Ok(Some(n)) => { if tw.write_all(&b2[..n]).await.is_err() { break; } }
                                                                }
                                                            }
                                                        }
                                                    );
                                                }
                                            });
                                        }
                                    });
                                    tunnel_tasks.insert(local_port, h);
                                }
                                Err(e) => {
                                    res_q.lock().unwrap().push(("端口映射".into(), false, format!("绑定 {local_port} 失败: {e}")));
                                }
                            }
                        }
                        SessionCmd::StopPortmap(local_port) => {
                            if let Some(h) = tunnel_tasks.remove(&local_port) {
                                h.abort();
                            }
                            portmap_rules.lock().unwrap().retain(|r| r.local_port != local_port);
                            sys_results.lock().unwrap().push(("端口映射".into(), true, format!("已停止 :{local_port}")));
                        }
                    }
                }
            }
        }
    }

    for (_, h) in tunnel_tasks {
        h.abort();
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

fn load_app_icon() -> Arc<egui::IconData> {
    let png_bytes = include_bytes!("../../../assets/icons/app-256.png");
    let img = image::load_from_memory(png_bytes)
        .expect("decode embedded app-256.png")
        .into_rgba8();
    let (width, height) = img.dimensions();
    Arc::new(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
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
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SessionCmd>();
    let sys_results: Arc<Mutex<Vec<(String, bool, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let monitors: Arc<Mutex<(Vec<MonitorDesc>, usize)>> = Arc::new(Mutex::new((Vec::new(), 0)));
    let remote_apps: Arc<Mutex<Vec<AppEntry>>> = Arc::new(Mutex::new(Vec::new()));
    let file_history: Arc<Mutex<Vec<TransferRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let portmap_rules: Arc<Mutex<Vec<PortmapRule>>> = Arc::new(Mutex::new(Vec::new()));
    let audio_enabled = Arc::new(AtomicBool::new(false));
    let gamepad_enabled = Arc::new(AtomicBool::new(false));
    let ca_override = Arc::new(Mutex::new(None));
    let sni_override = Arc::new(Mutex::new(None));

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
            cmd_rx,
            sys_results.clone(),
            monitors.clone(),
            remote_apps.clone(),
            file_history.clone(),
            portmap_rules.clone(),
            audio_enabled.clone(),
            gamepad_enabled.clone(),
            None,
            None,
            None,
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
        monitors,
        remote_apps,
        file_history,
        portmap_rules,
        audio_enabled,
        gamepad_enabled,
        ca_override,
        sni_override,
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("LeeConnect")
            .with_icon(load_app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "LeeConnect",
        opts,
        Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, sess, input_tx, ctrl_tx, cmd_tx, host, mode)) as Box<dyn eframe::App>)),
    )
}
