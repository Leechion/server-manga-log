use super::icons;
use super::shell;
use super::theme::{self, Palette};
use super::views::{connecting, idle, live, overlays};
use super::widgets;
pub use super::{FrameSlot, PortmapRule, Session, SessionCmd, Stage, Status, TransferRecord};
use egui::{pos2, vec2, Align, Color32, CornerRadius, Layout, Rect, Sense, Stroke};
use leeconnect::annotation::StrokePath;
use leeconnect::capabilities::Capabilities;
use leeconnect::input::InputEvent;
use leeconnect::msg::{AppEntry, ControlMsg, MonitorDesc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppMode {
    Normal,
    Gallery,
    TestScreens,
}

#[derive(Clone)]
pub struct DeviceRow {
    pub name: String,
    pub online: bool,
    pub kind: &'static str,
    pub addr: SocketAddr,
    pub device_id: Option<String>,
    pub public_addr: Option<SocketAddr>,
}

pub struct Toast {
    pub title: String,
    pub message: String,
    pub created: Instant,
}

#[derive(Serialize, Deserialize)]
pub struct UiConfig {
    pub dark_theme: bool,
    pub sidebar_open: bool,
    pub quality_preset: usize,
    pub sync_clipboard: bool,
    #[serde(default)]
    pub play_remote_audio: bool,
    #[serde(default = "default_true")]
    pub show_remote_cursor: bool,
    #[serde(default)]
    pub rotation_deg: i32,
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    #[serde(default)]
    pub prefer_hw_enc: bool,
    #[serde(default)]
    pub follow_mode: bool,
}

fn default_true() -> bool {
    true
}

fn default_zoom() -> f32 {
    1.0
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            dark_theme: false,
            sidebar_open: true,
            quality_preset: 1,
            sync_clipboard: true,
            play_remote_audio: false,
            show_remote_cursor: true,
            rotation_deg: 0,
            zoom: 1.0,
            prefer_hw_enc: false,
            follow_mode: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct LocalEnvCaps {
    pub nvenc: bool,
    pub mf_h264_hw: bool,
    pub mf_hevc_hw: bool,
    pub vigem: bool,
    pub mumu: bool,
}

pub struct GuiApp {
    pub host: SocketAddr,
    pub mode: AppMode,
    pub slot: Arc<Mutex<FrameSlot>>,
    pub status: Arc<Mutex<Status>>,
    pub caps: Arc<Mutex<Option<Capabilities>>>,
    pub hw: Arc<AtomicU64>,
    pub hh: Arc<AtomicU64>,
    pub stop: Arc<AtomicBool>,
    pub bytes_rx: Arc<AtomicU64>,
    pub rtt_ms: Arc<AtomicU64>,
    pub input_tx: Sender<InputEvent>,
    pub ctrl_tx: Sender<ControlMsg>,
    pub cmd_tx: Sender<SessionCmd>,
    pub sys_results: Arc<Mutex<Vec<(String, bool, String)>>>,
    pub monitors: Arc<Mutex<(Vec<MonitorDesc>, usize)>>,
    pub remote_apps: Arc<Mutex<Vec<AppEntry>>>,
    pub file_history: Arc<Mutex<Vec<TransferRecord>>>,
    pub portmap_rules: Arc<Mutex<Vec<PortmapRule>>>,
    pub audio_enabled: Arc<AtomicBool>,
    pub gamepad_enabled: Arc<AtomicBool>,
    pub ca_override: Arc<Mutex<Option<Vec<u8>>>>,
    pub sni_override: Arc<Mutex<Option<String>>>,
    pub pair_result: Arc<Mutex<Option<Result<crate::PairedTarget, String>>>>,
    pub pair_signal_addr: String,
    pub pair_in_flight: bool,
    pub device_list_result: Arc<Mutex<Option<Result<Vec<leeconnect::pair::OnlineDevice>, String>>>>,
    pub device_refreshing: bool,
    pub device_refresh_manual: bool,
    pub nat_probe_result: Arc<Mutex<Option<(leeconnect::ice::NatType, Option<SocketAddr>)>>>,
    pub nat_probing: bool,
    pub stun_server_input: String,
    pub relay_addr_input: String,
    pub env_caps: LocalEnvCaps,
    pub rtt_history: Vec<u64>,
    pub last_rtt_sample: Instant,
    pub tex: Option<egui::TextureHandle>,
    pub last_seq: u64,
    pub selected_device: usize,
    pub devices: Vec<DeviceRow>,
    pub palette: Palette,
    pub sidebar_open: bool,
    pub was_idle: bool,
    pub search_query: String,
    pub show_info_popover: bool,
    pub show_more_menu: bool,
    pub show_file_panel: bool,
    pub show_apps_modal: bool,
    pub show_portmap_modal: bool,
    pub show_power_modal: bool,
    pub show_monitor_popover: bool,
    pub show_net_modal: bool,
    pub pending_power_action: Option<(&'static str, &'static str)>,
    pub portmap_local_input: String,
    pub portmap_remote_input: String,
    pub upload_path_input: String,
    pub download_path_input: String,
    pub app_search_query: String,
    pub show_disconnect_confirm: bool,
    pub show_pair_modal: bool,
    pub pair_code_input: String,
    pub is_fullscreen: bool,
    pub quality_preset: usize,
    pub prefer_hw_enc: bool,
    pub sync_clipboard: bool,
    pub play_remote_audio: bool,
    pub show_remote_cursor: bool,
    pub privacy_screen_active: bool,
    pub relative_mouse_mode: bool,
    pub follow_mode: bool,
    pub annotation_mode: bool,
    pub annotation_color: [u8; 4],
    pub annotations: Vec<StrokePath>,
    pub active_stroke: Option<StrokePath>,
    pub last_follow_size: (u32, u32),
    pub last_follow_sent: Instant,
    pub rotation_deg: i32,
    pub zoom: f32,
    pub last_mouse_activity: Instant,
    pub session_start: Instant,
    pub toasts: Vec<Toast>,
    pub test_step: usize,
    pub test_frame: u32,
    pub shot_saved: Arc<AtomicBool>,
}

impl GuiApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        sess: Session,
        input_tx: Sender<InputEvent>,
        ctrl_tx: Sender<ControlMsg>,
        cmd_tx: Sender<SessionCmd>,
        host: SocketAddr,
        mode: AppMode,
    ) -> Self {
        theme::install_fonts(&cc.egui_ctx);

        let cfg = Self::load_config();
        sess.audio_enabled.store(cfg.play_remote_audio, Ordering::Relaxed);
        let palette = if cfg.dark_theme { theme::DARK } else { theme::LIGHT };
        theme::apply(&cc.egui_ctx, &palette);
        let env_caps = LocalEnvCaps {
            nvenc: leeconnect::encoder::is_nvenc_available(),
            mf_h264_hw: leeconnect::encoder::has_mf_h264_hw(),
            mf_hevc_hw: leeconnect::encoder::has_mf_hevc_hw(),
            vigem: leeconnect::gamepad::is_vigem_available(),
            mumu: leeconnect::mumu::is_mumu_available(),
        };

        let local_name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "本机电脑".to_owned());
        let mut app = Self {
            host,
            mode,
            slot: sess.slot,
            status: sess.status,
            caps: sess.caps,
            hw: sess.host_w,
            hh: sess.host_h,
            stop: sess.stop,
            bytes_rx: sess.bytes_rx,
            rtt_ms: sess.rtt_ms,
            sys_results: sess.sys_results,
            monitors: sess.monitors,
            remote_apps: sess.remote_apps,
            file_history: sess.file_history,
            portmap_rules: sess.portmap_rules,
            audio_enabled: sess.audio_enabled,
            gamepad_enabled: sess.gamepad_enabled,
            ca_override: sess.ca_override,
            sni_override: sess.sni_override,
            pair_result: Arc::new(Mutex::new(None)),
            pair_signal_addr: leeconnect::pair::DEFAULT_SIGNAL.to_owned(),
            pair_in_flight: false,
            device_list_result: Arc::new(Mutex::new(None)),
            device_refreshing: false,
            device_refresh_manual: false,
            nat_probe_result: Arc::new(Mutex::new(None)),
            nat_probing: false,
            stun_server_input: "223.5.5.5:3478".to_owned(),
            relay_addr_input: "127.0.0.1:9800".to_owned(),
            env_caps,
            rtt_history: vec![18; 24],
            last_rtt_sample: Instant::now(),
            input_tx,
            ctrl_tx,
            cmd_tx,
            tex: None,
            last_seq: 0,
            selected_device: 1, // Default selected is "工位电脑"
            devices: vec![
                DeviceRow {
                    name: local_name,
                    online: true,
                    kind: "本机",
                    addr: SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), super::DEFAULT_HOST),
                    device_id: None,
                    public_addr: None,
                },
                DeviceRow {
                    name: "工位电脑".into(),
                    online: true,
                    kind: "局域网 · 可连接",
                    addr: host,
                    device_id: None,
                    public_addr: None,
                },
                DeviceRow {
                    name: "LAPTOP-UJEGJK".into(),
                    online: false,
                    kind: "离线",
                    addr: SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 105)), super::DEFAULT_HOST),
                    device_id: None,
                    public_addr: None,
                },
            ],
            palette,
            sidebar_open: cfg.sidebar_open,
            was_idle: true,
            search_query: String::new(),
            show_info_popover: false,
            show_more_menu: false,
            show_file_panel: false,
            show_apps_modal: false,
            show_portmap_modal: false,
            show_power_modal: false,
            show_monitor_popover: false,
            show_net_modal: false,
            pending_power_action: None,
            portmap_local_input: "8080".to_owned(),
            portmap_remote_input: "80".to_owned(),
            upload_path_input: String::new(),
            download_path_input: String::new(),
            app_search_query: String::new(),
            show_disconnect_confirm: false,
            show_pair_modal: false,
            pair_code_input: String::new(),
            is_fullscreen: false,
            quality_preset: cfg.quality_preset,
            prefer_hw_enc: cfg.prefer_hw_enc,
            sync_clipboard: cfg.sync_clipboard,
            play_remote_audio: cfg.play_remote_audio,
            show_remote_cursor: cfg.show_remote_cursor,
            privacy_screen_active: false,
            relative_mouse_mode: false,
            follow_mode: cfg.follow_mode,
            annotation_mode: false,
            annotation_color: [255, 59, 48, 255],
            annotations: Vec::new(),
            active_stroke: None,
            last_follow_size: (0, 0),
            last_follow_sent: Instant::now(),
            rotation_deg: cfg.rotation_deg,
            zoom: cfg.zoom,
            last_mouse_activity: Instant::now(),
            session_start: Instant::now(),
            toasts: Vec::new(),
            test_step: 0,
            test_frame: 0,
            shot_saved: Arc::new(AtomicBool::new(false)),
        };
        if mode == AppMode::Normal {
            app.refresh_devices(false);
        }
        app
    }

    pub fn sync_texture(&mut self, ctx: &egui::Context) {
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

    pub fn ensure_dummy_texture(&mut self, ctx: &egui::Context) {
        if self.tex.is_some() {
            return;
        }
        let w = 1280;
        let h = 720;
        let mut pixels = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let dx = (x as f32 - w as f32 * 0.7) / (w as f32);
                let dy = (y as f32 - h as f32 * 0.2) / (h as f32);
                let dist = (dx * dx + dy * dy).sqrt();
                let factor = (1.0 - dist).clamp(0.0, 1.0);

                let r = (11.0 + factor * 35.0) as u8;
                let g = (21.0 + factor * 50.0) as u8;
                let b = (34.0 + factor * 80.0) as u8;
                pixels.extend_from_slice(&[r, g, b, 255]);
            }
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
        self.tex = Some(ctx.load_texture("remote-screen", img, egui::TextureOptions::LINEAR));
        self.hw.store(w as u64, Ordering::Relaxed);
        self.hh.store(h as u64, Ordering::Relaxed);
    }

    pub fn push_toast(&mut self, title: &str, msg: &str) {
        self.toasts.push(Toast {
            title: title.to_owned(),
            message: msg.to_owned(),
            created: Instant::now(),
        });
    }

    /// Send a system-action RPC to the host (e.g. "lock", "taskmgr", "reboot:confirm").
    pub fn send_system_action(&mut self, action: &str) {
        let _ = self.ctrl_tx.send(ControlMsg::SystemAction { action: action.to_owned() });
    }

    /// Drain host system-action results + completed pairing attempts into toasts.
    pub fn drain_sys_results(&mut self) {
        let dev_list_res = self.device_list_result.lock().unwrap().take();
        if let Some(res) = dev_list_res {
            self.device_refreshing = false;
            let manual = self.device_refresh_manual;
            self.device_refresh_manual = false;
            match res {
                Ok(list) => {
                    if let Some(self_row) = self.devices.first().cloned() {
                        let mut next = vec![self_row];
                        for d in list {
                            let kind = if d.public_endpoint.is_some() {
                                "在线 · P2P/中继"
                            } else {
                                "在线 · 局域网"
                            };
                            next.push(DeviceRow {
                                name: d.name,
                                online: true,
                                kind,
                                addr: d.lan_endpoint.unwrap_or(self.host),
                                device_id: Some(d.device_id),
                                public_addr: d.public_endpoint,
                            });
                        }
                        self.devices = next;
                        if self.devices.len() > 1 && self.selected_device == 0 {
                            self.selected_device = 1;
                        } else if self.selected_device >= self.devices.len() {
                            self.selected_device = self.devices.len().saturating_sub(1);
                        }
                        if manual {
                            self.push_toast(
                                "设备列表已刷新",
                                &format!("发现 {} 台在线设备", self.devices.len().saturating_sub(1)),
                            );
                        }
                    }
                }
                Err(e) => {
                    if manual {
                        self.push_toast("刷新设备失败", &e);
                    }
                }
            }
        }
        let pair_res = self.pair_result.lock().unwrap().take();
        if let Some(res) = pair_res {
            self.pair_in_flight = false;
            match res {
                Ok(pt) => {
                    let display_addr = pt.outcome.public_endpoint.unwrap_or(pt.outcome.lan_endpoint);
                    self.show_pair_modal = false;
                    self.pair_code_input.clear();
                    self.start_session_inner(display_addr, Some(pt));
                    self.push_toast("配对成功", &format!("正在连接至 {display_addr}（支持 P2P / 中继）"));
                }
                Err(e) => {
                    self.push_toast("配对失败", &e);
                }
            }
        }
        let nat_res = self.nat_probe_result.lock().unwrap().take();
        if let Some((nat_type, mapped)) = nat_res {
            self.nat_probing = false;
            let desc = match nat_type {
                leeconnect::ice::NatType::DirectOrOpen => "公网直连可达",
                leeconnect::ice::NatType::BehindNat => "处于 NAT 后（支持打洞 / 中继）",
                leeconnect::ice::NatType::Blocked => "STUN 探测超时（建议使用 UDP 中继）",
            };
            let addr_str = mapped.map(|a| format!(" ({a})")).unwrap_or_default();
            self.push_toast("NAT 网络探测", &format!("{desc}{addr_str}"));
            *self.nat_probe_result.lock().unwrap() = Some((nat_type, mapped));
        }
        if self.last_rtt_sample.elapsed().as_millis() >= 800 {
            self.last_rtt_sample = Instant::now();
            let rtt = self.rtt_ms.load(Ordering::Relaxed);
            if rtt > 0 {
                if self.rtt_history.len() >= 24 {
                    self.rtt_history.remove(0);
                }
                self.rtt_history.push(rtt);
            }
        }
        let pending = {
            let mut q = self.sys_results.lock().unwrap();
            if q.is_empty() {
                return;
            }
            std::mem::take(&mut *q)
        };
        for (action, ok, detail) in pending {
            let title = if ok { "操作成功" } else { "操作失败" };
            self.push_toast(title, &format!("{action} → {detail}"));
        }
    }

    /// Start async pairing code redemption against `signaling.js`.
    pub fn refresh_devices(&mut self, manual: bool) {
        if self.device_refreshing {
            return;
        }
        self.device_refreshing = true;
        self.device_refresh_manual = manual;
        let signal = self.pair_signal_addr.clone();
        let out = self.device_list_result.clone();
        std::thread::spawn(move || {
            let res = (|| -> anyhow::Result<Vec<leeconnect::pair::OnlineDevice>> {
                let mut sig = leeconnect::pair::Signaling::open(&signal)?;
                let serial = leeconnect::pair::gen_serial(leeconnect::pair::DEFAULT_SECRET, "LEE-CTRL-GUI");
                sig.register("LEE-CTRL-GUI", &serial, "lee-ctrl-gui", "", &[])?;
                sig.list_devices(None)
            })();
            *out.lock().unwrap() = Some(res.map_err(|e| e.to_string()));
        });
    }

    pub fn redeem_pair_code(&mut self) {
        if self.pair_in_flight || self.pair_code_input.len() != 6 {
            return;
        }
        self.pair_in_flight = true;
        let code = self.pair_code_input.clone();
        let signal = self.pair_signal_addr.clone();
        let stun_addr = leeconnect::ice::resolve_stun(&self.stun_server_input);
        let out = self.pair_result.clone();
        std::thread::spawn(move || {
            let res = (|| -> anyhow::Result<crate::PairedTarget> {
                let mut sig = leeconnect::pair::Signaling::open(&signal)?;
                let serial = leeconnect::pair::gen_serial(leeconnect::pair::DEFAULT_SECRET, "LEE-CTRL-GUI");
                sig.register("LEE-CTRL-GUI", &serial, "lee-ctrl-gui", "", &[])?;
                let outcome = sig.pair_redeem_full(&code)?;
                Ok(crate::PairedTarget {
                    sig,
                    outcome,
                    stun_addr,
                })
            })();
            *out.lock().unwrap() = Some(res.map_err(|e| e.to_string()));
        });
    }

    pub fn start_nat_probe(&mut self) {
        if self.nat_probing {
            return;
        }
        self.nat_probing = true;
        let addr_str = self.stun_server_input.clone();
        let out = self.nat_probe_result.clone();
        std::thread::spawn(move || {
            let res = match addr_str.parse::<SocketAddr>() {
                Ok(addr) => leeconnect::ice::detect_nat_posture(addr),
                Err(_) => (leeconnect::ice::NatType::Blocked, None),
            };
            *out.lock().unwrap() = Some(res);
        });
    }

    pub fn send_quality_config(&self) {
        let prof = leeconnect::encoder::QualityProfile::from_preset(self.quality_preset);
        let _ = self.ctrl_tx.send(ControlMsg::QualityConfig {
            bitrate_bps: prof.bitrate_bps,
            fps: prof.fps,
            chroma_444: prof.chroma == leeconnect::encoder::ChromaSampling::Yuv444,
            hdr: prof.hdr,
        });
    }

    pub fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.palette = if self.palette.dark { theme::LIGHT } else { theme::DARK };
        theme::apply(ctx, &self.palette);
        self.save_config();
    }

    pub fn stop_session(&mut self) {
        if self.follow_mode {
            let _ = self.ctrl_tx.send(ControlMsg::RestoreResolution);
        }
        let _ = self.ctrl_tx.send(ControlMsg::SystemAction { action: "end".into() });
        self.stop.store(true, Ordering::Relaxed);
        *self.status.lock().unwrap() = Status::Idle;
        self.tex = None;
        self.last_seq = 0;
        self.sidebar_open = true;
        self.relative_mouse_mode = false;
        self.annotation_mode = false;
    }

    pub fn reconnect(&mut self) {
        self.connect_to_selected();
    }

    pub fn connect_to_selected(&mut self) {
        *self.ca_override.lock().unwrap() = None;
        *self.sni_override.lock().unwrap() = None;
        let selected = self.devices.get(self.selected_device).cloned();
        if let Some(dev) = selected {
            if let Some(target_id) = dev.device_id {
                if self.pair_in_flight {
                    return;
                }
                self.pair_in_flight = true;
                let signal = self.pair_signal_addr.clone();
                let stun_addr = leeconnect::ice::resolve_stun(&self.stun_server_input);
                let out = self.pair_result.clone();
                std::thread::spawn(move || {
                    let res = (|| -> anyhow::Result<crate::PairedTarget> {
                        let mut sig = leeconnect::pair::Signaling::open(&signal)?;
                        let serial = leeconnect::pair::gen_serial(
                            leeconnect::pair::DEFAULT_SECRET,
                            "LEE-CTRL-GUI",
                        );
                        sig.register("LEE-CTRL-GUI", &serial, "lee-ctrl-gui", "", &[])?;
                        let outcome = sig.connect_device(&target_id)?;
                        Ok(crate::PairedTarget {
                            sig,
                            outcome,
                            stun_addr,
                        })
                    })();
                    *out.lock().unwrap() = Some(res.map_err(|e| e.to_string()));
                });
                return;
            }
            self.connect_addr(dev.addr);
            return;
        }
        self.connect_addr(self.host);
    }

    pub fn connect_addr(&mut self, target_addr: SocketAddr) {
        self.start_session_inner(target_addr, None);
    }

    pub fn start_session_inner(
        &mut self,
        target_addr: SocketAddr,
        paired_target: Option<crate::PairedTarget>,
    ) {
        self.stop.store(true, Ordering::Relaxed);
        self.tex = None;
        self.last_seq = 0;
        self.stop = Arc::new(AtomicBool::new(false));
        self.audio_enabled.store(self.play_remote_audio, Ordering::Relaxed);

        *self.status.lock().unwrap() = Status::Connecting {
            stage: Stage::Handshake,
            started: Instant::now(),
            done: Vec::new(),
        };

        let (tx, input_rx) = std::sync::mpsc::channel::<InputEvent>();
        self.input_tx = tx;
        let (ctx_tx, ctrl_rx) = std::sync::mpsc::channel::<ControlMsg>();
        self.ctrl_tx = ctx_tx;
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SessionCmd>();
        self.cmd_tx = cmd_tx;
        self.sys_results.lock().unwrap().clear();
        self.monitors.lock().unwrap().0.clear();
        self.remote_apps.lock().unwrap().clear();
        self.portmap_rules.lock().unwrap().clear();

        // Sync initial audio & quality settings once stream starts
        let _ = self.ctrl_tx.send(ControlMsg::AudioToggle { enabled: self.play_remote_audio });
        self.send_quality_config();

        let ca_der = self.ca_override.lock().unwrap().clone();
        let sni = self.sni_override.lock().unwrap().clone();

        Session::spawn(
            target_addr,
            self.slot.clone(),
            self.status.clone(),
            self.caps.clone(),
            self.hw.clone(),
            self.hh.clone(),
            self.stop.clone(),
            self.bytes_rx.clone(),
            self.rtt_ms.clone(),
            input_rx,
            ctrl_rx,
            cmd_rx,
            self.sys_results.clone(),
            self.monitors.clone(),
            self.remote_apps.clone(),
            self.file_history.clone(),
            self.portmap_rules.clone(),
            self.audio_enabled.clone(),
            self.gamepad_enabled.clone(),
            ca_der,
            sni,
            paired_target,
        );
    }

    pub fn config_path() -> Option<PathBuf> {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("LeeConnect").join("ui.json"))
    }

    pub fn load_config() -> UiConfig {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save_config(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let cfg = UiConfig {
                dark_theme: self.palette.dark,
                sidebar_open: self.sidebar_open,
                quality_preset: self.quality_preset,
                sync_clipboard: self.sync_clipboard,
                play_remote_audio: self.play_remote_audio,
                show_remote_cursor: self.show_remote_cursor,
                rotation_deg: self.rotation_deg,
                zoom: self.zoom,
                prefer_hw_enc: self.prefer_hw_enc,
                follow_mode: self.follow_mode,
            };
            if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}

pub fn save_screenshot(img: &egui::ColorImage, path: impl AsRef<Path>) {
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity(w * h * 4);
    for px in &img.pixels {
        raw.extend_from_slice(&[px.r(), px.g(), px.b(), px.a()]);
    }
    if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, raw) {
        if let Some(p) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let _ = buf.save(path.as_ref());
        println!("[screenshot] Saved to {}", path.as_ref().display());
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));

        if self.mode == AppMode::TestScreens {
            self.run_test_step(ui);
            return;
        }

        self.sync_texture(ui.ctx());
        self.drain_sys_results();

        // Unified macOS App Shell
        self.render_app_shell(ui);

        // Overlay modals, popovers, menus, and toasts
        overlays::render(self, ui.ctx());
    }
}

impl GuiApp {
    pub fn render_app_shell(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let st = self.status.lock().unwrap().clone();
        let is_live = matches!(st, Status::Live);

        // Auto-collapse sidebar on entering live session
        if is_live && self.was_idle {
            self.sidebar_open = false;
            self.was_idle = false;
        } else if !is_live {
            self.was_idle = true;
        }

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

            // 1. Top Unified Toolbar (52px)
            let dev_name = self.devices.get(self.selected_device).map(|d| d.name.as_str()).unwrap_or("工位电脑");
            let is_online = self.devices.get(self.selected_device).map_or(false, |d| d.online);
            let status_sub = if is_live {
                // live latency breakdown (UU feature: delay_reason):
                // host capture+encode, wire RTT, local decode — all measured, no fakes.
                let (cap_us, enc_us, dec_us) = {
                    let s = self.slot.lock().unwrap();
                    (s.host_capture_us, s.host_encode_us, s.decode_us)
                };
                let rtt = self.rtt_ms.load(Ordering::Relaxed);
                let total = cap_us as f64 / 1000.0 + enc_us as f64 / 1000.0 + rtt as f64 + dec_us as f64 / 1000.0;
                format!(
                    "已连接 · 采集 {:.1}ms · 编码 {:.1}ms · 网络 {}ms · 解码 {:.1}ms · 合计 {:.0}ms",
                    cap_us as f64 / 1000.0,
                    enc_us as f64 / 1000.0,
                    rtt,
                    dec_us as f64 / 1000.0,
                    total
                )
            } else if is_online {
                "在线 · 局域网 · Windows 11".to_owned()
            } else {
                "离线".to_owned()
            };

            let action = shell::render_toolbar(
                ui,
                &p,
                dev_name,
                &status_sub,
                is_online || is_live,
                is_live,
                self.sidebar_open,
                self.show_info_popover,
                self.show_net_modal,
                &mut self.search_query,
            );

            if self.mode != AppMode::TestScreens {
                if action.toggle_sidebar {
                    self.sidebar_open = !self.sidebar_open;
                }
                if action.toggle_info {
                    self.show_info_popover = !self.show_info_popover;
                }
                if action.toggle_theme {
                    self.toggle_theme(ui.ctx());
                }
                if action.toggle_net {
                    self.show_net_modal = !self.show_net_modal;
                }
            }

            // 2. Main Content Area (Sidebar + Content Stage)
            let avail_h = ui.available_height();
            let target_w = if self.sidebar_open { 224.0 } else { 0.0 };
            let anim_w = if self.mode == AppMode::TestScreens {
                target_w
            } else {
                ui.ctx().animate_value_with_time(
                    ui.scope_id().with("sidebar-animated-width"),
                    target_w,
                    0.28,
                )
            };

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                if anim_w > 1.0 {
                    ui.allocate_ui_with_layout(
                        vec2(anim_w, avail_h),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_height(avail_h);
                            ui.set_max_height(avail_h);
                            self.render_sidebar_content(ui, anim_w, is_live);
                        },
                    );
                }

                let content_w = ui.available_width();
                let content_start = ui.cursor().min;
                ui.allocate_ui_with_layout(
                    vec2(content_w, avail_h),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_height(avail_h);
                        ui.set_max_height(avail_h);
                        let content_rect = Rect::from_min_size(content_start, vec2(content_w, avail_h));
                        let bg = if is_live { p.live_bg } else { p.content };
                        ui.painter().rect_filled(content_rect, CornerRadius::ZERO, bg);
                        match &st {
                            Status::Live => {
                                live::render_body(self, ui);
                            }
                            Status::Connecting { stage, started, .. } => {
                                connecting::render_body(self, ui, *stage, *started);
                            }
                            Status::Idle | Status::Closed | Status::Failed(_) => {
                                idle::render_body(self, ui);
                            }
                        }
                    },
                );
            });
        });
    }

    fn render_sidebar_content(&mut self, ui: &mut egui::Ui, width: f32, is_live: bool) {
        let p = self.palette;
        let avail_h = ui.available_height();
        let (rect, _) = ui.allocate_exact_size(vec2(width, avail_h), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::ZERO, p.side);
        ui.painter().line_segment(
            [pos2(rect.max.x, rect.min.y), pos2(rect.max.x, rect.max.y)],
            Stroke::new(1.0, p.sep),
        );

        let mut inner_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(vec2(10.0, 14.0)))
                .layout(Layout::top_down(Align::Min)),
        );

        inner_ui.set_width((width - 20.0).max(10.0));
        inner_ui.vertical(|ui| {
            // Section 1: 「我的设备」 + 刷新按钮
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("我的设备")
                        .font(theme::semibold_font(11.0))
                        .color(p.text_3),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if self.device_refreshing { "刷新中…" } else { "刷新" };
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(label)
                                    .font(theme::text_font(11.0))
                                    .color(p.accent),
                            )
                            .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        self.refresh_devices(true);
                    }
                });
            });
            ui.add_space(4.0);

            let query = self.search_query.to_lowercase();
            let mut toast_msg = None;
            for (idx, dev) in self.devices.iter().enumerate() {
                if !query.is_empty() && !dev.name.to_lowercase().contains(&query) {
                    continue;
                }
                let is_self = dev.kind.contains("本机");
                let is_laptop = dev.name.to_lowercase().contains("laptop");
                let icon = if is_self {
                    icons::PC_CASE
                } else if is_laptop {
                    icons::LAPTOP
                } else {
                    icons::MONITOR
                };

                let sel = self.selected_device == idx;
                if widgets::side_item(ui, &p, icon, &dev.name, dev.online, is_self, sel).clicked() {
                    if is_live {
                        toast_msg = Some("请先断开当前会话再切换设备");
                    } else {
                        self.selected_device = idx;
                    }
                }
            }
            if let Some(msg) = toast_msg {
                self.push_toast("提示", msg);
            }

            ui.add_space(14.0);

            // Section 2: 「其他设备」
            ui.label(
                egui::RichText::new("其他设备")
                    .font(theme::semibold_font(11.0))
                    .color(p.text_3),
            );
            ui.add_space(4.0);

            // Add device link
            let add_resp = ui.horizontal(|ui| {
                icons::draw_icon(
                    ui.painter(),
                    pos2(ui.cursor().min.x + 8.0, ui.cursor().min.y + 12.0),
                    icons::PLUS,
                    15.0,
                    p.text_2,
                );
                ui.add_space(20.0);
                ui.link(egui::RichText::new("添加设备…").font(theme::text_font(12.5)).color(p.text_2))
            });
            if add_resp.inner.clicked() {
                self.show_pair_modal = true;
            }
        });
    }

    fn run_test_step(&mut self, ui: &mut egui::Ui) {
        self.test_frame += 1;

        // Steps definitions for macOS UI:
        // (dark, is_live, is_connecting, info_popover, more_menu, pair_sheet, disconnect_alert, filename)
        let steps = [
            // 0. Idle (Light)
            (false, false, false, false, false, false, false, "01-idle-light.png"),
            // 1. Connecting (Light)
            (false, false, true, false, false, false, false, "02-connecting-light.png"),
            // 2. Live Session (Light)
            (false, true, false, false, false, false, false, "03-live-light.png"),
            // 3. Live + More Menu (Light)
            (false, true, false, false, true, false, false, "04-live-more-menu-light.png"),
            // 4. Live + Info Popover (Light)
            (false, true, false, true, false, false, false, "05-live-info-popover-light.png"),
            // 5. Pair Code Sheet Modal (Light)
            (false, false, false, false, false, true, false, "06-pair-sheet-light.png"),
            // 6. Disconnect Alert Modal (Light)
            (false, true, false, false, false, false, true, "07-disconnect-alert-light.png"),
            // 7. Idle (Dark)
            (true, false, false, false, false, false, false, "08-idle-dark.png"),
            // 8. Live Session (Dark)
            (true, true, false, false, false, false, false, "09-live-dark.png"),
        ];

        if self.test_step >= steps.len() {
            println!("[test-screens] All 9 screenshots generated successfully!");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let (dark, is_live, is_connecting, info_pop, more_menu, pair_sheet, disconnect_alert, filename) = steps[self.test_step];

        // Configure state
        self.palette = if dark { theme::DARK } else { theme::LIGHT };
        theme::apply(ui.ctx(), &self.palette);
        self.show_info_popover = info_pop;
        self.show_more_menu = more_menu;
        self.show_pair_modal = pair_sheet;
        self.show_disconnect_confirm = disconnect_alert;

        if is_live {
            *self.status.lock().unwrap() = Status::Live;
            self.ensure_dummy_texture(ui.ctx());
            self.rtt_ms.store(18, Ordering::Relaxed);
            self.bytes_rx.store(14_250_000, Ordering::Relaxed);
            self.sidebar_open = false;
        } else if is_connecting {
            *self.status.lock().unwrap() = Status::Connecting {
                stage: Stage::Negotiate,
                started: Instant::now(),
                done: vec![(Stage::Handshake, Duration::from_millis(16))],
            };
            self.sidebar_open = true;
        } else {
            *self.status.lock().unwrap() = Status::Idle;
            self.sidebar_open = true;
        }

        self.render_app_shell(ui);
        overlays::render(self, ui.ctx());

        if self.shot_saved.load(Ordering::SeqCst) {
            self.shot_saved.store(false, Ordering::SeqCst);
            self.test_step += 1;
            self.test_frame = 0;
            return;
        }

        // Wait 25 frames for any animations/transitions to fully settle
        if self.test_frame == 25 {
            let out_path = format!("target/ui-macos/{}", filename);
            let ctx = ui.ctx().clone();
            let flag = self.shot_saved.clone();
            ctx.request_screenshot(move |image| {
                save_screenshot(&image, &out_path);
                flag.store(true, Ordering::SeqCst);
            });
        }
    }
}
