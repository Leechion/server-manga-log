# LeeConnect 功能复刻规格（源自 UU远程 GameViewer 逆向提取）

> 目标：从零实现一个功能对等的远程桌面软件，**不依赖 UU、不破解 UU**。
> 功能清单来源：`%LOCALAPPDATA%\GameViewer\cache\config_*.json` 的 `controlled_ability_map`
> （60+ feature，从 UU 二进制逆向提取，作为需求规格而非抄袭）。
> 每个 feature 映射到 LeeConnect 的一个模块 + 一个能力位，主控端按对端能力位协商降级。

## A. 视频链路
| UU feature | LeeConnect 模块 | 实现 |
|---|---|---|
| quality_normal / fps_90 / fps_144 | `host::capture` + `codec` | DXGI Desktop Duplication / WGC 采集 → Media Foundation H.264/HEVC 硬编（NVENC/QSV/AMF），帧率上限可配 |
| screen_super / manual_super_screen | `codec::super` | 高码率档（自定义 GOP/CRF） |
| screen_hdr | `codec::hdr` | 10bit + HDR10 传输函数 |
| screen_chroma | `codec` | 4:4:4 色度采样档 |
| screen_zoomscale / screen_orientation | `client::render` | 缩放/旋转适配 |
| desktop_display / windows_display / multi_screen | `host::display` | 多显示器枚举 + 每屏独立流 |
| screen_virtual | `host::idd` | 虚拟显示器（IDD 驱动，复刻 GameViewerIddDriver 能力） |
| full_screen | `client::render` | 全屏渲染（wgpu/D3D11） |
| delay_reason | `core::qos` | 延迟归因（采集/编码/网络/解码/渲染分段计时） |
| custom_bitrate | `core::config` | 码率手调 |

## B. 输入链路
| UU feature | LeeConnect 模块 | 实现 |
|---|---|---|
| device_mouse / device_keyboard | `host::inject` | SendInput（绝对/相对坐标） |
| mouse_cursor / smart_mouse / simulate_mouse / virtual_mouse | `host::cursor` | 光标同步 + 相对模式（游戏） |
| input_protocol_v1 | `core::proto` | 输入帧协议（含滚轮/中键/侧键/组合键） |
| device_gamepad / pad_touch | `host::vigem` | ViGEmBus 虚拟手柄转发 |
| combination_keys（ctrl+alt+del 等） | `host::secure` | 安全注意序列（SAS）注入 |
| device_task_manager / device_lock / device_reboot / device_shutdown | `host::system` | 系统动作 RPC |
| watch_unlock / auto_unlock / disconnect_lock | `host::lock` | 锁屏联动 |

## C. 音频链路
| UU feature | LeeConnect 模块 | 实现 |
|---|---|---|
| voice_setting / device_microphone | `host::audio` | WASAPI loopback（系统声）+ 麦克风采集 → Opus |
| remote_mute | `core::config` | 远端静音开关 |

## D. 数据链路
| UU feature | LeeConnect 模块 | 实现 |
|---|---|---|
| file_trans_v1..v4 / manual_transfer | `host::fs` | 双向文件传输（QUIC 流，断点续传） |
| remote_clipboard_v1..v5 | `host::clipboard` | 剪贴板双向同步（文本/图片/文件） |
| remote_app_list / remote_connection_cli_v2 | `host::apps` | 应用列表 + 快速启动 |
| port_mapping | `host::proxy` | TCP 端口映射隧道 |
| follow_local / follow_remote | `core::session` | 跟随模式 |

## E. 隐私/安全
| UU feature | LeeConnect 模块 | 实现 |
|---|---|---|
| screen_privacy / screen_privacy_enhanced / privacy_wallpaper | `host::privacy` | 隐私屏（黑屏/壁纸 + 本地输入屏蔽） |
| remote_assist_verify / verification_code | `core::pair` | 配对码 + 验证类型 |
| mumu_control / virtual_plan | `host::cloud` | 云手机控制（可选，对应 nemu-api） |
| wow_detection | `core::nat` | 网络/NAT 类型探测 |

## F. 连接底座（LeeConnect 自研，替代 UU 云）
- **信令**：`server/signaling.js`（Node，已有骨架）——配对码、SDP/能力协商、心跳、relay 分配
- **传输**：`core::quic`（Rust `quinn`）——多流复用：视频流/输入流/文件流/控制流，0-RTT、弱网抗丢包
- **打洞**：`core::ice`——STUN/ICE，失败回落 `server/relay.js`
- **鉴权**：`server/auth.js`——本地 SERIAL（HMAC），无云端依赖

## 里程碑（端到端可验证，逐个交付）
- [x] **M1 协议核心**（12/12 单测通过，含真实桌面采集）：
  `leeconnect/src/{frame,capabilities,msg,transport,capture}.rs`
  - 帧协议 LEECV1 + 能力位图（24 位覆盖 60+ UU feature 归并，协商=交集）
  - 控制消息 JSON（type tag 对齐 Node signaling.js）
  - QUIC 传输（quinn 0.11 + rcgen 自签 + 客户端 pin CA 全验证 TLS1.3）：回环 3 帧混合 + 4 路并发流
  - DXGI Desktop Duplication 采集：真实抓 2560×1440 BGRA，DPI-aware 修正物理分辨率
- [~] **M2 本机回环视频**（闭环跑通，采集已实证真实内容）：
  `capture -> RawEncoder -> QUIC -> decode -> frame0.ppm/png` 端到端验证，~8 fps 无损回环
  - ✅ 采集/编码/传输/解码/存盘全链路通（`cargo run --bin loopback`）
  - ✅ **采集内容已肉眼验证**：`prime()` 刷新初始空帧后，抓到真实桌面（壁纸/任务栏/窗口/对话框），
    诊断 mean-luma 172、nonblack-pixels 100%，PNG 3.1MB（非纯黑）。之前"首帧全黑"是未 prime 的陈旧判断，已修正
  - ⚠️ 目前用 RawEncoder 无损（147MB/10帧 ≈ 14.7MB/帧），**未接 Media Foundation H.264 硬编**
    （下一步；`bgra_to_nv12` 转换已备好，接上后单帧可降到 ~100KB 级）
- [x] **M3 输入闭环 + 双进程会话**（2026-11-23 实测通过）：
  `leeconnect/src/input.rs` + `src/bin/{host,ctrl}.rs`
  - SendInput 鼠标/键盘/滚轮/组合键注入，InputEvent 编解码往返通过
  - **真实双进程会话**：`host.exe`（推视频+注入输入）↔ `ctrl.exe`（收视频+发输入），单条多路复用 QUIC 流
    - 视频 host→ctrl：12 帧，mean-luma 129.6 / nonblack 100%（真实桌面）
    - 输入 ctrl→host：5 事件，host `injected 5`（SendInput 全接受）
    - 能力协商：host 声明 caps=198156，ctrl 正确解析
    - 跨进程信任：host `gen_server_config_persisted` 发布 CA → ctrl `load_ca` pin 同根，TLS1.3 握手成功
- [x] **M5 数据链路**（2026-11-23 实测通过，26/26 全绿）：
  `leeconnect/src/{file,clipboard,portmap}.rs`
  - **文件传输**（UU file_trans_v1..v4/manual_transfer）：分块协议 Meta/Chunk/Done/Req，256KB 块；
    QUIC 端到端 1.25MB 伪随机文件 → 接收 → **逐字节比对通过**
    - 关键修复：QUIC `open_bi` 本地分配流、须**首字节写出**才激活对端 `accept_bi`（先发 Req）；
      server 任务须 `conn.closed().await` 挂住，否则提前 drop 重置连接丢数据
  - **剪贴板同步**（UU remote_clipboard_v1..v5）：GlobalAlloc+SetClipboardData 写 / GetClipboardData+GlobalLock 读，
    CF_UNICODETEXT；**真实剪贴板写后读回验证通过**。坑：GlobalUnlock 单次锁下返回 FALSE 但 GetLastError=NO_ERROR=成功
  - **端口映射**（UU port_mapping）：TCP-over-QUIC 隧道，controller 本地监听→每连接开一条 QUIC 双向流→host 转发到目标 TCP，
    双向泵；**echo 往返测试通过**（client→ctrl TCP→QUIC→host→echo server→原路返回）
- [x] **MF/H.264 编码**（2026-11-23 实测通过，30/30 全绿）：
  `leeconnect/src/encoder.rs::H264Encoder`（openh264 软编，Cisco 编解码器纯 Rust 封装）
  - 真实桌面 2560×1440 帧：无损 14400 KB → **H.264 98 KB（145.9x 压缩）**，keyframe=IDR
  - **编解码往返验证**：solid 帧 encode→decode→像素还原（灰色偏差 ≤24，符合有损容差）
  - Annex-B 起始码校验、强制关键帧、尺寸守卫、stride-pitched 重打包（防跨行读填充）
  - 双进程会话实测：host 走 H.264 路径，12 帧仅 0.1MB（无损时 176.9MB，~1700x）
  - 选型说明：本机无 nasm + 纯 MF H264 MFT 是数百行 COM 样板，先用 openh264 软编打通；
    NVENC/MF 硬编作后续优化（同 `VideoEncoder` trait，替换实现即可，能力位协商选路）
  - 注：UsageType::ScreenContentRealTime 下 openh264 自动关闭 AQ/BackgroundDetection（未支持），无害
- [x] **完整视频链路 + 解码还原**（2026-11-23 实测通过，32/32 全绿）：
  `leeconnect/src/decoder.rs`（openh264 解码）+ ctrl 接入
  - **双进程端到端**：host DXGI采集→H.264编码→QUIC→ctrl H.264解码→BGRA还原存盘
    实测 `h264=true`，12 帧 0.1MB，还原 2560×1440 nonblack 99.9%（肉眼验证 decoded.png 清晰可读）
  - 编解码往返单测：gradient 帧 encode→decode 尺寸/pts/内容（luma 区间）校验通过
  - 至此 **UU 的核心远程桌面能力（看+控）已用自研 Rust 全链路复刻**：
    采集/编码/传输/解码/还原 + 输入注入 + 文件/剪贴板/端口映射，均实测
- [ ] **实时渲染上屏**：解码帧显示到窗口（wgpu/D3D11；当前存盘验证，缺 GUI 呈现）
- [ ] **M6 音频**：WASAPI loopback + Opus
- [ ] **M4 双机 LAN**：127.0.0.1 换真实两机 IP + Node 信令配对码接入
- [ ] **M7 高级**：多屏/HDR/虚拟手柄(ViGEm)/隐私屏/IDD 虚拟屏
- [ ] **M8 NAT 穿透 + 中继**：QUIC hole punching + relay 兜底
- [ ] **NVENC/MF 硬编**：替换 openh264 软编（同 VideoEncoder trait，能力协商选路）

## 当前可运行产物
```
cargo run --bin loopback -- 10          # M2 单进程采集-编码-QUIC-解码-存盘回环
# M3 双进程会话：
cargo run --bin host                     # 终端A：监听 127.0.0.1:44330，推流+注入
cargo run --bin ctrl                     # 终端B：连接，收流+发输入，存 target/session/frame0.ppm
```

## 工程环境（已验证）
Rust 1.100 (E:\rust) + MSVC Build Tools + Windows SDK 26100 + crates.io 可达。
坑位记录：`CC` 环境变量残留 w64devkit gcc 会劫持 cc-rs → 编译前 `Remove-Item Env:CC`；
quinn 0.11 `ServerConfig::with_single_cert` 不设 ALPN（client 侧也不能设）；
`max_concurrent_bidi_streams` 收 `VarInt`；测试里 server 任务须 `conn.closed().await` 挂住防止 endpoint 提前 drop 重置连接。

## 法律/道德边界（重要）
复刻 = 用**自己的代码**实现**同类功能**，参照的是功能清单（idea，不受版权保护），
**不复制 UU 的任何代码/资源/商标**。逆向仅用于理解协议语义以做互斥参考，
LeeConnect 走完全自研协议。前面那套 hosts/假云"会员破解"已废弃——UU 本就免费，无意义。
