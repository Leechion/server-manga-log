//! LeeConnect GUI client — a UU-remote-style desktop app (egui/eframe).
//! Top bar (brand + connection state), left device nav, center live remote
//! screen, bottom toolbar. Mouse over the screen streams absolute input to the
//! host; clicking sends a left-button down/up pair.
//!
//! Run: cargo run --bin host   (terminal A)
//!      cargo run --bin gui    (terminal B)   [-- host_ip:port]

use anyhow::Result;
use leeconnect::capabilities::Capabilities;
use leeconnect::decoder::H264Decoder;
use leeconnect::encoder::{EncodedFrame, PixelFormat};
use leeconnect::frame::{Frame, FrameType};
use leeconnect::input::{encode_input, InputEvent, MouseButton};
use leeconnect::msg::{self, ControlMsg};
use leeconnect::transport;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

const DEFAULT_HOST: u16 = 44330;

/// Latest decoded remote frame, shared network-thread -> UI thread.
#[derive(Default)]
struct FrameSlot {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    seq: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Connecting,
    Live,
    Closed,
}

struct Session {
    slot: Arc<Mutex<FrameSlot>>,
    status: Arc<Mutex<Status>>,
    caps: Arc<Mutex<Option<Capabilities>>>,
    host_w: Arc<AtomicU64>,
    host_h: Arc<AtomicU64>,
}

impl Session {
    /// Spawn the background network thread (own current-thread tokio runtime).
    fn spawn(
        host: SocketAddr,
        slot: Arc<Mutex<FrameSlot>>,
        status: Arc<Mutex<Status>>,
        caps: Arc<Mutex<Option<Capabilities>>>,
        hw: Arc<AtomicU64>,
        hh: Arc<AtomicU64>,
        input_rx: Receiver<InputEvent>,
    ) {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[gui-session] runtime: {e}");
                    return;
                }
            };
            if let Err(e) = rt.block_on(run(host, slot, status, caps, hw, hh, input_rx)) {
                eprintln!("[gui-session] ended: {e}");
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
    input_rx: Receiver<InputEvent>,
) -> Result<()> {
    let ca = transport::load_ca().unwrap_or_default();
    let client = transport::client_endpoint(&ca)?;
    let conn = match client.connect(host, transport::SNI) {
        Ok(c) => match c.await {
            Ok(conn) => conn,
            Err(e) => {
                *status.lock().unwrap() = Status::Closed;
                anyhow::bail!("handshake {host}: {e}");
            }
        },
        Err(e) => {
            *status.lock().unwrap() = Status::Closed;
            anyhow::bail!("connect {host}: {e}");
        }
    };
    let (mut send, mut recv) = conn.open_bi().await?;
    *status.lock().unwrap() = Status::Live;

    let mut decoder = H264Decoder::new()?;
    let mut seq = 0u32;
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(16));
    loop {
        tokio::select! {
            r = transport::read_frame(&mut recv) => {
                let f = match r { Ok(f) => f, Err(_) => break };
                match f.ftype {
                    FrameType::Video => {
                        let ef = EncodedFrame::from_wire(&f.payload)?;
                        hw.store(ef.width as u64, Ordering::Relaxed);
                        hh.store(ef.height as u64, Ordering::Relaxed);
                        if ef.format == PixelFormat::H264 {
                            if let Some(pic) = decoder.decode(&ef)? {
                                let mut s = slot.lock().unwrap();
                                s.w = pic.width;
                                s.h = pic.height;
                                bgra_to_rgba_into(&pic.pixels, &mut s.rgba);
                                s.seq += 1;
                            }
                        } else if ef.format == PixelFormat::Bgra8 {
                            let mut s = slot.lock().unwrap();
                            s.w = ef.width;
                            s.h = ef.height;
                            bgra_to_rgba_into(&ef.data, &mut s.rgba);
                            s.seq += 1;
                        }
                    }
                    FrameType::Control => {
                        let txt = String::from_utf8_lossy(&f.payload).to_string();
                        if let Ok(ControlMsg::CapabilityExchange { caps: c }) = msg::decode(&txt) {
                            *caps.lock().unwrap() = Some(Capabilities::from_u64(c));
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
            }
        }
    }
    *status.lock().unwrap() = Status::Closed;
    Ok(())
}

/// BGRA (decoder output) -> RGBA (egui texture) into `out`.
fn bgra_to_rgba_into(bgra: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(bgra.len());
    for px in bgra.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
}

// ---- egui application ----

struct DeviceRow {
    name: String,
    online: bool,
    kind: &'static str,
}

struct GuiApp {
    slot: Arc<Mutex<FrameSlot>>,
    status: Arc<Mutex<Status>>,
    caps: Arc<Mutex<Option<Capabilities>>>,
    hw: Arc<AtomicU64>,
    hh: Arc<AtomicU64>,
    input_tx: Sender<InputEvent>,
    tex: Option<egui::TextureHandle>,
    last_seq: u64,
    selected_device: usize,
    devices: Vec<DeviceRow>,
    fonts_installed: bool,
}

/// Load a system CJK font as a fallback so Chinese labels render (egui default
/// font has no CJK glyphs -> tofu boxes). Appended after the Latin default.
fn install_cjk_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\Deng.ttf",
        "C:\\Windows\\Fonts\\NotoSansSC-VF.ttf",
    ];
    let bytes = match CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) {
        Some(b) => b,
        None => return,
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&fam) {
            list.push("cjk".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

impl GuiApp {
    fn new(_cc: &eframe::CreationContext<'_>, sess: Session, input_tx: Sender<InputEvent>) -> Self {
        Self {
            slot: sess.slot,
            status: sess.status,
            caps: sess.caps,
            hw: sess.host_w,
            hh: sess.host_h,
            input_tx,
            tex: None,
            last_seq: 0,
            selected_device: 0,
            devices: vec![
                DeviceRow { name: "DESKTOP-M2SOF0N".into(), online: true, kind: "本机" },
                DeviceRow { name: "工位电脑".into(), online: true, kind: "局域网" },
                DeviceRow { name: "LAPTOP-UJEGJK".into(), online: false, kind: "离线" },
                DeviceRow { name: "MacBook".into(), online: false, kind: "离线" },
            ],
            fonts_installed: false,
        }
    }

    /// Push the newest decoded frame into an egui texture (create once, update after).
    fn sync_texture(&mut self, ctx: &egui::Context) {
        let (new_rgba, w, h, seq) = {
            let s = self.slot.lock().unwrap();
            if s.seq == self.last_seq || s.rgba.is_empty() {
                return;
            }
            (s.rgba.clone(), s.w as usize, s.h as usize, s.seq)
        };
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &new_rgba);
        match &mut self.tex {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => {
                self.tex =
                    Some(ctx.load_texture("remote-screen", img, egui::TextureOptions::LINEAR));
            }
        }
        self.last_seq = seq;
    }

    fn status_text(&self) -> (&'static str, egui::Color32) {
        match *self.status.lock().unwrap() {
            Status::Connecting => ("连接中…", egui::Color32::YELLOW),
            Status::Live => ("● 已连接", egui::Color32::from_rgb(40, 200, 90)),
            Status::Closed => ("○ 未连接", egui::Color32::LIGHT_RED),
        }
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.fonts_installed {
            install_cjk_fonts(ui.ctx());
            self.fonts_installed = true;
        }
        // repaint ~60fps so live video updates without user input
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        self.sync_texture(ui.ctx());

        // ---- top bar ----
        let (stxt, scol) = self.status_text();
        let caps_bits = self.caps.lock().unwrap().map(|c| c.bits());
        egui::Panel::top("topbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("LeeConnect");
                ui.separator();
                ui.colored_label(scol, stxt);
                if let Some(bits) = caps_bits {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("caps {bits}")).small().weak());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("LeeOwner");
                });
            });
        });

        // ---- bottom toolbar ----
        egui::Panel::bottom("tools").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (icon, label) in [
                    ("⏻", "断开"),
                    ("⛶", "全屏"),
                    ("📋", "剪贴板"),
                    ("📁", "文件"),
                    ("🔌", "端口映射"),
                    ("🎮", "手柄"),
                    ("🔒", "锁屏"),
                    ("🖥", "隐私屏"),
                ] {
                    ui.button(format!("{icon} {label}"));
                }
            });
        });

        // ---- left device nav ----
        egui::Panel::left("nav").default_size(210.0).show(ui, |ui| {
            ui.add_space(4.0);
            ui.strong("我的设备");
            ui.add_space(4.0);
            for (i, d) in self.devices.iter().enumerate() {
                let dot = if d.online { "●" } else { "○" };
                let col = if d.online {
                    egui::Color32::from_rgb(40, 200, 90)
                } else {
                    egui::Color32::GRAY
                };
                let label = egui::RichText::new(format!("{dot}  {}", d.name)).color(col);
                ui.selectable_value(&mut self.selected_device, i, label);
                ui.label(
                    egui::RichText::new(format!("      {}", d.kind)).small().weak(),
                );
            }
        });

        // ---- center: live remote screen ----
        egui::CentralPanel::default().show(ui, |ui| {
            let hw = self.hw.load(Ordering::Relaxed) as usize;
            let hh = self.hh.load(Ordering::Relaxed) as usize;
            let tex = match &self.tex {
                Some(t) if hw > 0 && hh > 0 => t,
                _ => {
                    ui.centered_and_justified(|ui| {
                        ui.label("等待远端画面…（请先启动 host，再运行本客户端）");
                    });
                    return;
                }
            };

            let avail = ui.available_size();
            let scale = (avail.x / hw as f32).min(avail.y / hh as f32);
            let disp = egui::vec2(hw as f32 * scale, hh as f32 * scale);
            let (rect, _r) = ui.allocate_exact_size(disp, egui::Sense::click_and_drag());
            ui.put(rect, egui::Image::new(tex).max_size(disp));

            let id = ui.id().with("remote-input");
            let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pos) && rect.width() > 1.0 && rect.height() > 1.0 {
                    let rx = ((pos.x - rect.min.x) / rect.width() * hw as f32)
                        .clamp(0.0, hw as f32 - 1.0) as u32;
                    let ry = ((pos.y - rect.min.y) / rect.height() * hh as f32)
                        .clamp(0.0, hh as f32 - 1.0) as u32;
                    let _ = self.input_tx.send(InputEvent::MoveAbs { x: rx, y: ry });
                    if resp.clicked() {
                        let _ = self
                            .input_tx
                            .send(InputEvent::Button { button: MouseButton::Left, down: true });
                        let _ = self
                            .input_tx
                            .send(InputEvent::Button { button: MouseButton::Left, down: false });
                    }
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    let host_str = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("127.0.0.1:{DEFAULT_HOST}"));
    let host: SocketAddr = host_str
        .parse()
        .unwrap_or_else(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_HOST));

    let slot = Arc::new(Mutex::new(FrameSlot::default()));
    let status = Arc::new(Mutex::new(Status::Connecting));
    let caps = Arc::new(Mutex::new(None));
    let hw = Arc::new(AtomicU64::new(0));
    let hh = Arc::new(AtomicU64::new(0));
    let (input_tx, input_rx) = std::sync::mpsc::channel::<InputEvent>();

    Session::spawn(
        host,
        slot.clone(),
        status.clone(),
        caps.clone(),
        hw.clone(),
        hh.clone(),
        input_rx,
    );

    let sess = Session {
        slot,
        status,
        caps,
        host_w: hw,
        host_h: hh,
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("LeeConnect"),
        ..Default::default()
    };
    eframe::run_native(
        "LeeConnect",
        opts,
        Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, sess, input_tx)) as Box<dyn eframe::App>)),
    )
}
