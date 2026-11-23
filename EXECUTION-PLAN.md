# LeeConnect 剩余功能执行清单

> 目标：把 UU远程（GameViewer）的全部功能复刻为自研 LeeConnect（Rust + Node）。
> 本清单只列**剩余未完成项**；已完成基线见文末。每项含：对应 UU feature / 落地模块 / 技术方案 / 验收标准 / 依赖 / 规模(S<M<L<XL)。
> 更新：2026-11-23。技术栈：Rust 1.100 + MSVC + Windows SDK 26100 + quinn 0.11 + openh264 + windows 0.58 + Node。

---

## P0 — 让软件"成形可用"（最高优先）

### 1. 实时渲染上屏 ✅（2026-11-23 完成并实测）
- **落地**：`leeconnect/src/bin/gui.rs`（egui/eframe 0.37，glow 后端）
- **实测**：host+gui 双进程，gui 窗口中央实时显示 host 真实桌面（截图 `leeconnect/target/gui-window.png`）；
  左侧设备导航（我的设备/本机/工位电脑/离线）中文正常（CJK 字体已加载，`gui-cjk.png`）；
  顶栏连接状态绿点 + caps 显示；底部工具栏（断开/全屏/剪贴板/文件/端口映射/手柄/锁屏/隐私屏）；
  鼠标悬停/点击 → 绝对坐标+左键事件回传 host（MoveAbs/Button 已实现）
- **待深化**（不影响"客户端已成形"结论）：工具栏按钮动作接线（属 #2/#6/#7 范围）、键盘捕获（当前仅鼠标）
- **依赖坑记录**：cargo schannel+HTTP2 多路复用握手失败 → `CARGO_HTTP_MULTIPLEXING=false`（已固化进 build.ps1）；
  egui 0.37 API 与旧版差异大（`App::ui`、`Panel::top/left`、`allocate_exact_size` 返回 `(Rect,Response)`、
  `Image::new(&TextureHandle)`、`TextureHandle::set`、字体 `families` 追加回退）——均按 crate 源码核实
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

### 3. 双机 LAN 连接
- **UU feature**：`remote_connection_cli` / `remote_connection_cli_v2` / 配对码
- **模块**：`bin/host.rs` `bin/ctrl.rs` bind 地址参数化；接入 `server/signaling.js`
- **方案**：
  - host/ctrl 支持 `--listen <ip:port>` / `--connect <ip:port>`，去掉硬编码 127.0.0.1
  - 配对码流程：host 向 Node 信令 `pair_create` 拿码 → ctrl `pair_redeem` → 信令交换 QUIC 端点 → 直连（复用 `msg.rs` ControlMsg，type tag 已对齐 signaling.js）
  - 防火墙：首次运行 `netsh advfirewall` 放行 UDP（或提示用户）
- **验收**：两台真实机器经配对码建立会话，画面+输入互通
- **依赖**：#1 渲染（体验）；信令骨架已存在
- **规模**：M

---

## P1 — 功能对齐（远程桌面核心体验）

### 4. 多显示器
- **UU feature**：`multi_screen` / `screen_super`
- **模块**：`capture.rs` 扩展 `DesktopDup::for_output(idx)`，枚举 `EnumOutputs(0..n)`
- **方案**：每屏一个 duplication，独立编码流（channel 区分）；ctrl 选屏
- **验收**：≥2 屏机器可分别采集/切换显示
- **依赖**：#1
- **规模**：S

### 5. 光标同步与相对模式
- **UU feature**：`mouse_cursor` / `smart_mouse` / `simulate_mouse` / `virtual_mouse`
- **模块**：`input.rs` + `capture.rs`（`GetCursorInfo` 取形状/位置随帧下发）
- **方案**：
  - 光标渲染：host 每帧附光标位置+位图，ctrl 叠加绘制（或 host 端已合成，简单）
  - 相对模式（游戏）：Raw Input（`WM_INPUT` / `RIDEV`）捕获相对位移，`InputEvent::MoveRel` 已支持
- **验收**：ctrl 看到实时光标；相对模式下 FPS 游戏可转向
- **依赖**：#1
- **规模**：M

### 6. 系统动作 RPC
- **UU feature**：`device_reboot` / `device_shutdown` / `device_lock` / `device_task_manager` / `watch_unlock` / `auto_unlock` / `disconnect_lock`
- **模块**：`bin/host.rs` 处理 `ControlMsg::SystemAction`（协议已预留）
- **方案**：`windows` 调用 `ExitWindowsEx`（重启/关机）、`LockWorkStation`（锁屏）、`WTS` 断开锁定；taskmgr 用 `CreateProcess`
- **验收**：ctrl 触发各动作，host 正确执行（关机/重启需确认保护，测试用锁屏/taskmgr）
- **依赖**：#3（信令通道）
- **规模**：S

### 7. 隐私屏
- **UU feature**：`screen_privacy` / `screen_privacy_enhanced` / `privacy_wallpaper`
- **模块**：新增 `leeconnect/src/privacy.rs`
- **方案**：全屏黑窗置顶（`CreateWindowEx` WS_EX_TOPMOST）+ 低级钩子屏蔽本地输入（`SetWindowsHookExW` WH_KEYBOARD_LL/WH_MOUSE_LL，需消息循环）；远端仍可看
- **验收**：host 物理屏变黑且本地键鼠无效，ctrl 画面不受影响
- **依赖**：无
- **规模**：M

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

### 12. 缩放 / 旋转
- **UU feature**：`screen_zoomscale` / `screen_orientation`
- **方案**：ctrl 端渲染变换（缩放矩阵 / 旋转），或 host 端 `ChangeDisplaySettingsEx` 改分辨率
- **验收**：画面可缩放、竖屏旋转正确
- **依赖**：#1
- **规模**：S

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

### 16. 延迟归因
- **UU feature**：`delay_reason`
- **方案**：采集/编码/传输/解码/渲染分段打时间戳，控制帧回传统计，ctrl 显示各段耗时
- **验收**：UI 显示分段延迟分解
- **依赖**：#1
- **规模**：S

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
| **UU 式 GUI 客户端** | `bin/gui.rs` | egui/eframe，实时远控画面+设备导航+工具栏+中文，实测截图 |
| 双进程会话 | `bin/{host,ctrl}.rs` | 看+控端到端 |

**测试**：32/32 全绿。核心"看 + 控 + 传"已闭环可用。

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
