# LeeConnect 剩余功能执行清单

> 目标：把 UU远程（GameViewer）的全部功能复刻为自研 LeeConnect（Rust + Node）。
> 本清单只列**剩余未完成项**；已完成基线见文末。每项含：对应 UU feature / 落地模块 / 技术方案 / 验收标准 / 依赖 / 规模(S<M<L<XL)。
> 更新：2026-11-23。技术栈：Rust 1.100 + MSVC + Windows SDK 26100 + quinn 0.11 + openh264 + windows 0.58 + Node。

---

## P0 — 让软件"成形可用"（最高优先）

### 1. 实时渲染上屏 ✅（2026-11-23 完成并实测）
- **落地**：`leeconnect/src/bin/gui/`（模块化：`main.rs`, `app.rs`, `theme.rs`, `widgets.rs`, `views/`，egui/eframe 0.37，glow 后端）
- **实测**：host+gui 双进程，gui 窗口中央实时显示 host 真实桌面；
  按 `docs/UI-REDESIGN-execution-plan.md` 完整重构为 BoardUI 规范 v2：
  - M0：单文件 `gui.rs` 拆分为 `src/bin/gui/` 模块架构；
  - M1：浅色/深色双套 Token 调色板，Inter Medium/Regular 字体与 Windows CJK 回退；
  - M2：自绘 24x24 纯几何矢量图标库与复用组件（tray, card, status_pill, chip, btn, button_group, segmented, toggle, device_row, step_row）；
  - M3：布局重排（设备顶栏、60px/236px 浮动圆角侧栏、三组底栏工具箱、可收起指标右栏、中央托盘及等比映射）；
  - M4：Stage/Status 状态机，三阶段耗时连接进度、失败错误弹窗、断开二次确认与 6 位配对码；
  - M5：会话设置面板、文件传输窗口、Toast 通知堆叠与延迟分解；
  - M6：浅色/深色与 6 种强调色切换，`%APPDATA%\LeeConnect\ui.json` 自动持久化；
  - M7：自动化视觉验收套件（`--test-screens`, `--gallery`），9 个高保真画面截图已固化至 `leeconnect/target/ui-v2/`。
- **待深化**（不影响"客户端已成形"结论）：工具栏按钮动作接线（属 #2/#6/#7 范围）、键盘捕获（当前仅鼠标）
- **依赖坑记录**：cargo schannel+HTTP2 多路复用握手失败 → `CARGO_HTTP_MULTIPLEXING=false`（已固化进 build.ps1）；
  egui 0.37 API 与旧版差异大（`App::ui`、`Panel::exact_size`、`allocate_exact_size` 返回 `(Rect,Response)`、
  `Context::viewport_rect()`、`Context::request_screenshot`、字体 `families` 追加回退）——均按 crate 源码核实
- **运行**：`.\build.ps1 run --bin gui`（host 需先启动）

### 2. 音频链路
- **UU feature**：`voice_setting` / `device_microphone` / `remote_mute` / audio
- **模块**：新增 `leeconnect/src/audio.rs`（host 采集编码 + ctrl 解码播放）
- **方案**：
  - 采集：WASAPI loopback（系统声）+ 麦克风，`windows` Core Audio（`IAudioClient`/`IAudioCaptureClient`，feature `Win32_Media_Audio_Endpoints`）
  - 编码：Opus，`audiopus` crate（libopus 绑定）
  - 传输：`FrameType::Audio`（协议已预留）
  - 播放：WASAPI render，`windows` `IAudioClient`
- **验收**：host 播放系统声音 → ctrl 端扬声器听到，延迟 <150ms；静音开关生效
- **依赖**：无
- **规模**：L

### 3. 双机 LAN 连接 ✅（2026-11-24 完成并实测）
- **UU feature**：`remote_connection_cli` / `remote_connection_cli_v2` / 配对码
- **落地**：`src/pair.rs`（新：tungstenite 信令客户端 + Rust 侧 SERIAL 生成，与 Node keygen
  **逐字节兼容**交叉单测；RFC4648 base32 向量单测；local_ip UDP 路由探测）+
  `transport.rs::gen_server_config_extra`（leaf SAN 加本机 LAN IP → 按 IP 连接仍全链名称验证）+
  `signaling.js` register 携带 endpoint/ca、pair_ok 回传 endpoint+CA +
  `host.rs --pair [signal]`（注册→打印 6 位码→等待连入）+ `ctrl.rs --pair <code> [signal]`
- **实测**：本机全链路——信令起 → host 出码 `858431` → ctrl 凭码解析出 `192.168.5.109:44330` →
  QUIC 握手（IP-SAN 名称验证）→ 12 帧 H.264 解码还原 + 5 输入注入，会话闭环
- **规模**：M ✅

---

## P1 — 功能对齐（远程桌面核心体验）

### 4. 多显示器 ✅（2026-11-24 完成并实测）
- **UU feature**：`multi_screen` / `screen_super`
- **落地**：`capture.rs` 新增 `DesktopDup::new_for_output(idx)` + `monitor_count()`；
  分辨率改用 `IDXGIOutput::GetDesc` 的 DesktopCoordinates（物理像素，替代 GDI primary_resolution，
  副屏/竖屏不再错配）；`bin/host.rs` 支持 `--monitor <idx>` 与 `--list-monitors`，
  ≥2 屏时能力位自动加 `MULTI_SCREEN`
- **实测**：本机 2 输出（#0 2560×1440、#1 1800×3200 竖屏）；`--monitor 1` 启动后
  host 报 `desktop #1 1440x2560` 且 H.264 编码正常出帧（竖屏走 DXGI 原生旋转坐标）
- **待深化**：ctrl/GUI 端运行时切屏（需信令消息 + 每屏独立流），当前为 host 启动参数选屏
- **规模**：S ✅

### 5. 光标同步与相对模式
- **UU feature**：`mouse_cursor` / `smart_mouse` / `simulate_mouse` / `virtual_mouse`
- **落地**：`src/cursor.rs`（新，9 字节线格式 + GetCursorInfo poll，4 单测含真实轮询）+
  `frame.rs::FrameType::Cursor`（协议扩展）+ host 随每视频帧下发光标 + gui/main.rs 接收存 FrameSlot +
  live.rs 叠加绘制（白十字+黑晕，`show_remote_cursor` 开关控制，坐标按远端分辨率映射）
- **实测**：三端编译通过；host→ctrl 端到端视频流正常（Cursor 帧零破坏，decoded.png 605KB 真实画面）
- **相对模式**：`InputEvent::MoveRel` 协议已支持，host 注入端已实现；GUI 端 Raw Input 捕获（游戏转向）待接
- **待深化**：光标形状位图（GetFramePointerShape 需持有帧时取，M7 增强）；GUI 相对模式开关
- **规模**：M ✅（位置同步完成，形状/相对模式列入后续）

### 6. 系统动作 RPC ✅（2026-11-24 完成，34/34 单测）
- **UU feature**：`device_reboot` / `device_shutdown` / `device_lock` / `device_task_manager` / `watch_unlock` / `auto_unlock` / `disconnect_lock`
- **落地**：`src/system.rs`（新，Action::parse 安全门 + execute）+ `msg.rs::SystemActionResult` 回执 + `bin/host.rs` Control 分支执行回传 + `bin/gui/` ctrl_tx 通道 → toast
- **动作**：`lock`（LockWorkStation）、`taskmgr`（CREATE_NO_WINDOW）、`logoff/reboot/shutdown`（ExitWindowsEx | EWX_FORCEIFHUNG）
- **安全门**：破坏性动作必须 `:confirm` 后缀，parse 阶段拒绝（5 单测：安全动词/确认要求/token 严格/未知动词/handle 门先于执行）
- **GUI**：「锁定远程电脑」菜单从伪造 Win+L 按键改为真实 RPC + 回执 toast；协议往返单测覆盖 `system_action_result` tag
- **备注**：真 Ctrl+Alt+Del（SendSAS）需 sas.dll + 注册表策略 + 提权，windows-0.58 无绑定 → 保留按键注入近似，列入 P1 后续
- **验收**：单测 34/34；host/gui 编译通过；端到端锁屏可人工点菜单验证
- **规模**：S ✅

### 7. 隐私屏 ✅（2026-11-24 完成，4 单测 + 三端编译）
- **UU feature**：`screen_privacy` / `screen_privacy_enhanced` / `privacy_wallpaper`
- **落地**：`src/privacy.rs`（新）——全屏虚拟桌面黑窗置顶（CreateWindowExW WS_EX_TOPMOST|TOOLWINDOW，
  WM_KILLFOCUS 重置顶防抢焦点）+ WH_KEYBOARD_LL/WH_MOUSE_LL 低级钩子；钩子按 `LLMHF_INJECTED`
  区分：**吞物理输入、放行远端 SendInput**（故 ctrl 画面与控制不受影响）
- **安全后门**：1.5s 内连按 3 次 Esc 本地解除（`backdoor_fires` 纯函数，防锁死）；
  `should_block`/`backdoor` 4 个纯逻辑单测覆盖
- **接线**：host Control 分支 `privacy_on`/`privacy_off` 动词（在 system::handle 前拦截）；
  GUI live.rs 隐私屏按钮从空转变为真实 RPC + toast
- **验证边界**：全屏黑屏真实起停未纳入自动化（会短时屏蔽物理输入，避免干扰），以逻辑单测+三端编译为证；
  人工可经 GUI 按钮实测
- **规模**：M ✅

### 8. 虚拟手柄转发
- **UU feature**：`device_gamepad` / `pad_touch`
- **模块**：新增 `leeconnect/src/gamepad.rs`
- **方案**：ViGEmBus 虚拟 Xbox 手柄（`vigem` crate，或直调 `ViGEmClient.dll`）；ctrl 手柄输入 → `FrameType::Input` 扩展 gamepad 事件 → host 注入虚拟手柄
- **验收**：host 出现虚拟 Xbox 控制器，ctrl 手柄输入驱动之
- **依赖**：系统装 ViGEmBus 驱动（UU 已带，或引导安装）
- **规模**：M

---

## P2 — 画质进阶

### 9. NVENC / MF 硬编
- **UU feature**：`quality_normal` / `fps_90` / `fps_144` / `custom_bitrate`
- **模块**：`encoder.rs` 新增 `NvencEncoder`/`MfH264Encoder`，实现 `VideoEncoder` trait
- **方案**：能力协商选路（有硬编用硬编，否则 openh264 软编）。NVENC 走 `nv-codec-headers`（`windows` + Nvenc C ABI）或 Media Foundation `IMFTransform`（H264 encoder MFT，COM 样板重）
- **验收**：硬编启用时 CPU 占用显著下降，≥60fps@1440p
- **依赖**：GPU 环境探测
- **规模**：L

### 10. HDR / 高位深
- **UU feature**：`screen_hdr`
- **模块**：`capture.rs`（DXGI 10bit 格式）+ `encoder.rs`（HEVC/Main10）
- **方案**：采集 `DXGI_FORMAT_R10G10B10A2_UNORM`，编码 HEVC-10（openh264 不支持，需 x265/`vidcoder` 或 MF）
- **验收**：HDR 显示器上色彩正确
- **依赖**：#9（HEVC 编码）
- **规模**：XL

### 11. 色度采样档
- **UU feature**：`screen_chroma`
- **方案**：编码参数暴露 4:2:0 / 4:4:4 档，openh264 支持有限，或走 MF/x264
- **验收**：文字边缘在 4:4:4 下清晰
- **依赖**：#9
- **规模**：M

### 12. 缩放 / 旋转 ✅（2026-11-24 完成，62/62 单测）
- **UU feature**：`screen_zoomscale` / `screen_orientation`
- **落地**：`src/view.rs`（新，纯几何：Rotation 枚举 + rotated_size/fit_size +
  pointer_to_remote 逆映射 + remote_to_pointer 正映射，10 单测含 4 方向正逆往返互逆证明）+
  `bin/gui/views/live.rs` 接入：Image::rotate 绘制 + 光标正映射 + 指针逆映射回传远端坐标 +
  `UiConfig`/`GuiApp` 增 rotation_deg/zoom 字段并持久化 ui.json
- **关键正确性**：旋转下鼠标点选经 `pointer_to_remote` 反旋回源坐标，远端命中位置准确；
  光标叠加经 `remote_to_pointer` 正旋，与画面同步（往返单测 `forward_inverse_roundtrip_all_rotations` 锁定）
- **实测**：三端编译通过；host→ctrl 端到端 12 帧解码还原正常（旋转逻辑在纯函数层已充分单测，
  绘制路径编译验证；GUI 交互旋转待人工点选确认）
- **解锁**：#15 跟随模式（依赖本项的分辨率/缩放基建）
- **规模**：S ✅

---

## P3 — 网络与增值

### 13. NAT 穿透 + 中继兜底
- **UU feature**：`wow_detection`（NAT 类型探测）
- **模块**：新增 `leeconnect/src/ice.rs`；接入 `server/relay.js`（LEEC 帧已定义）
- **方案**：STUN 探测（`stun-rs` 或自实现）→ QUIC 打洞（`quinn` 双向 connect）→ 失败走 relay.js UDP 中继
- **验收**：跨 NAT（不同路由器）两台机器直连或中继连通
- **依赖**：#3
- **规模**：L

### 14. 应用列表 / 快速启动
- **UU feature**：`remote_app_list` / `remote_connection_cli`
- **方案**：host 枚举开始菜单/注册表卸载项（`windows` Shell + 注册表），下发列表；ctrl 点选 → host `CreateProcess` 启动
- **验收**：ctrl 看到 host 应用列表并可远程启动
- **依赖**：#3
- **规模**：M

### 15. 跟随模式
- **UU feature**：`follow_local` / `follow_remote`
- **方案**：ctrl 本地分辨率变化时通知 host `ChangeDisplaySettingsEx` 对齐
- **验收**：ctrl 窗口缩放 host 分辨率跟随
- **依赖**：#12
- **规模**：S

### 16. 延迟归因 ✅（2026-11-24 完成，45/45 单测）
- **UU feature**：`delay_reason`
- **落地**：`src/stats.rs`（新，8 字节线格式 capture_us/encode_us，3 单测）+ `frame.rs::FrameType::Stats`
  + host 视频泵内实测 capture/encode 耗时随流下发 + gui/main.rs 接收存 FrameSlot 并实测本地 decode 耗时
  + `app.rs` 顶栏 `status_sub` 由**硬编码假值 "18ms"** 改为真实五段分解：
  `采集 x.xms · 编码 x.xms · 网络 xms(QUIC RTT) · 解码 x.xms · 合计 xms`
- **实测**：三端编译通过；host→ctrl 端到端视频流正常（Stats 帧零破坏，12 帧解码还原）
- **设计**：不共享绝对时钟，每段均为单机耗时差值，可安全相加
- **规模**：S ✅

### 17. 批注
- **UU feature**：`annotation_v1` / `annotation_v2`
- **方案**：ctrl 渲染层叠加画笔图层，坐标回传 host（或仅 ctrl 本地叠加）
- **验收**：可在画面上画线标注
- **依赖**：#1
- **规模**：M

### 18. 云手机控制
- **UU feature**：`mumu_control` / `virtual_plan`
- **方案**：对接 MuMu 模拟器控制 API（逆向见 `nemu-api.dll`），作为被控源之一
- **验收**：可连接并控制云手机实例
- **依赖**：#3
- **规模**：L（可选，非桌面核心）

---

## 验证与运行

```powershell
# 全量单测（含真实桌面采集/H.264编解码往返/剪贴板/文件/端口映射）
cd E:\CODE\server-manga-log\leeconnect
.\build.ps1 test --lib -- --include-ignored

# 双进程会话（本机回环）
.\build.ps1 run --bin host      # 终端A
.\build.ps1 run --bin ctrl      # 终端B：还原画面存 target/session/decoded.png

# 单进程采集-编码-QUIC-解码-存盘回环
.\build.ps1 run --bin loopback -- 10
```

> `build.ps1` 已封装：清除 `CC/CXX` 环境变量污染（w64devkit gcc 会劫持 cc-rs 编译 ring/openh264-sys2）。

---

## 已完成基线（勿重复实现）

| 能力 | 模块 | 实测 |
|---|---|---|
| 帧协议 LEECV1 | `frame.rs` | 编解码 + 错误路径单测 |
| 能力位图协商 | `capabilities.rs` | 交集协商 + 前向兼容 |
| 控制消息 JSON | `msg.rs` | 与 signaling.js type tag 对齐 |
| QUIC 传输 | `transport.rs` | 多路复用 + 自签 CA pin，TLS1.3 |
| 桌面采集 | `capture.rs` | DXGI Duplication，2560×1440 真实帧 |
| H.264 编码 | `encoder.rs::H264Encoder` | openh264，145.9× 压缩，98KB/帧 |
| H.264 解码 | `decoder.rs::H264Decoder` | 还原画面肉眼可辨 |
| 输入注入 | `input.rs` | SendInput 鼠标/键盘/滚轮/组合键 |
| 文件传输 | `file.rs` | 1.25MB 逐字节比对通过 |
| 剪贴板同步 | `clipboard.rs` | 真实写后读回验证 |
| 端口映射 | `portmap.rs` | TCP-over-QUIC echo 往返 |
| **UU 式 GUI 客户端** | `bin/gui/`（模块化） | macOS 风重构，实时远控画面+设备导航+工具栏+CJK，实测截图 |
| 双进程会话 | `bin/{host,ctrl}.rs` | 看+控端到端 |
| **系统动作 RPC** | `system.rs` + `SystemActionResult` | lock/taskmgr/logoff/reboot/shutdown，`:confirm` 安全门，5 单测 |
| **多显示器** | `capture.rs::new_for_output` + `--monitor` | 本机 2 屏实测（含 1440×2560 竖屏 H.264） |
| **光标同步** | `cursor.rs` + `FrameType::Cursor` | 随帧下发+GUI 叠加十字，4 单测+端到端 |

**测试**：42/42 全绿（含真实桌面采集/H.264 往返/剪贴板/文件/端口映射/光标轮询）。

---

## 依赖清单（待引入 crate）
- 渲染：`wgpu` + `winit`（或 `windows` Direct3D11）
- 音频：`audiopus`（libopus）+ `windows` Core Audio
- 手柄：`vigem`（或直调 ViGEmClient.dll）
- NAT：`stun-rs`（或自实现 STUN）
- 硬编：`nv-codec-headers` 或 MF `IMFTransform`
- HEVC（HDR）：`vidcoder`/x265 或 MF

## 风险与备注
- **openh264 无 nasm**：本机已用其预编译静态库绕过；换机需确认 `source` feature
- **MF H264 MFT 硬编**：COM 样板重（媒体类型协商/样本池/flush 事件），风险最高，软编已够用
- **隐私屏钩子**：低级钩子需消息循环，勿阻塞主线程
- **系统动作**：关机/重启测试需二次确认，避免误操作
- **合规**：全程自研代码，仅参照 UU 功能清单（idea），不复制其代码/资源/商标
