# LeeConnect RE-NOTES（真实 UU 样本逆向台账 · GameViewer 版）

> 样本来源：本机安装 `D:\Program Files\Netease\GameViewer`（注册表 `SOFTWARE\Netease\GameViewerSetup`，
> 包标识 `com.netease.uuremote`，运行进程 GameViewer.exe + crashpad_handler）。
> 副本归档于 `sandbox/uu/real/`（SHA256 见 patches.json），原始安装目录只读。
> 证据优先级：运行时行为 → 抓包流量 → 当前加载资产 → 进程配置 → 持久化状态 → 生成物 → 源码 → 注释。

## 0. 样本台账

| 二进制 | 角色 | 大小 | SHA256(前16) | 壳 |
|---|---|---|---|---|
| GameViewer.exe | 主控客户端 | 74.6 MB | 0E909C0FC654474B | 无（MSVC pe32+, RTTI 完整） |
| GameViewerServer.exe | 被控/串流 host | 32.5 MB | 1EA6480E91D2C5EC | 无，10 节区，IB=0x140000000 |
| GameViewerService.exe | 系统服务 | 8.1 MB | 1EA3B26EFED0D040 | — |
| GameViewerLauncher/Healthd.exe | 拉起/健康守护 | ~10 MB | 14F9160133A3B27A / 22174B8316872870 | — |
| Qt5*.dll + libssl-3-x64.dll | UI/网络栈 | — | — | OpenSSL 3（pinning 弱，mitm 可行） |
| drivers/ | IddDriver 虚拟显示、nevaudio 音频、uuycinput 输入、ViGEmBus 手柄 | — | — | 远控四件套 |

`nemu-api.dll` = MuMu 云手机 API（非 p2p 引擎）。aria2c.exe 做下载。WebView2 混合 UI。
版本 4.48.1.5606（HKCU\Software\Netease\GameViewer）。

## 1. 权益模型（已证实，三通道）

1. **云端 JSON**：`GET https://api.nrd.nie.163.com/api/v1/member/entitlements`（+ `/member/trial`,
   `/user/info`）→ `is_member / member_type / memberExpiredTime`
   - 解析器：`host::LoginManager::queryMemberInfo::<lambda_1>(const shared::business_api::GetUserInfoResponse&)`
   - 日志簇：`.rdata+0x16c3fa0`（`member_info result=updated member_type= ... is_member=`）
2. **本地特性 INI（零二进制注入点）**：`config\server\<设备hash>.ini` → `[super_screen] open=0`
   - 4 台设备实测全 0 = 非会员状态；CFG-001 改 `open=1` 即开超级屏
3. **能力配置表**：`POST /api/v1/tool/configures_with_version` → `controlled_ability_map`
   （fps_144/screen_super/simulate_mouse… 60+ feature，`final_version` 非空=版本锁），
   缓存落 `%LOCALAPPDATA%\GameViewer\cache\config_*.json`（含 `member_privilege`）

**位图 gate**：xref 扫描证明 `f6 c3 02/04/10`（test bl, bitmask）+ `80 bd f8 00 00 00 00`（byte flag @struct+0xF8）
调用簇：Server rva `0x3a32a0/0x39f7e0/0x38ba90/0x3ab560`，Client rva `0x5c7f10/0x5c9840/0x5d6ae0/0x5df680`。

## 2. 反制对抗（已证实存在）

- 反调试：`IsDebuggerPresent / OutputDebugStringW` 导入命中（Server 8 处、Client 16 处）
- 反虚机：`Microsoft Hv / vmware / virtualbox` cpuid/smbios 探测串（VM 里跑样本需 ANTI_VM_SNIPPET）
- 完整性：`runtime_product_identity result=fallback stage=signature reason=verification_failed`
  + `runtime_product_verification result=skipped reason=relaxed_environment`
  → 校验函数簇 Server rva `0x287db0–0x288a10`、Client rva `0x1b06010–0x1b06d20`，
  5 处条件分支（cmp_flag_je×3 + test_r12b_je + test_dil_je）已由 PC-002 NOP 化
- `.slog` 日志：SLOG 魔数封装 + 高熵密文正文（结构未破，不影响权益路径，暂搁置）

## 3. 信令面（api-map.json 回填来源）

`https://api.nrd.nie.163.com`（+api-ipv4/api-dcdn/api-event/fcount）API 全表已从 strings 提取：
- 会话：`/api/v1/app/cpc_device/preinstall/configure`, `/api/v1/room/notify/user/`, guest 房间系列
- 配对/验证：`remote_assist_verify_type / remote_assist_push / verification_code_rng`（验证码+控制 ID 语义）
- 设备：`/api/v1/device/*`（list/controllable/bind/screensaver/wol/share 全系）
- 登录：`/api/v1/login/by_mobile|by_qrcode|by_uu_token`
- 埋点：`/api/v1/event/report/batch` + `sentry.netease.com/69` DSN
- **信令长连接后台：`uuyc.163.com` / `uuyc.webapp.163.com`**（早期误全量接管导致设备列表空/`-100`）

## 4. 媒体面基线

- codec 能力缓存 `config/streamer/{encoder,decoder}_codec_capability_cache.json`：
  H264(1)/H265(2)，4K@HEVC 10bit，NVENC(codec_impl=0) + impl=5，chroma 420/444
- 编码器：NVENC 家族 + 软编；ICE：`ice config update requested mode=`（自研 P2P 栈，非标准 libwebrtc）

## 5. Phase 4 品牌化目标面

49 处 brand 串（UU/Netease/网易）+ `com.netease.uuremote` + 注册表键 + 驱动名(uuyc/nevaudio)
+ WebView2 html 资源 + .rsrc 图标/位图。`tools/rebrand.js`（Node 高速版）等长替换 59+105 处；
不等长项（UU 短词、RCData 图标）SKIP 清单在案，走 ResourceHacker lane。

## 6. 最终接管架构（选择性 MITM + 响应改写，2026-11-23 定稿）

假云不再是"全量应答"，而是**选择性 MITM**——保留真实账号与设备，只篡改权益字段：

```
客户端 --hosts--> 127.0.0.1:443 (LeeConnect mock-cloud, SYSTEM 计划任务自启)
   ├─ GET/POST /api/v1/member/*  ─────┐
   ├─ POST /api/v1/tool/configures ───┤→ 本地应答：member_type=3 + 17.6KB 全解锁能力表
   │                                  ┘  (simulate_mouse 等 final_version 锁标记清空)
   ├─ GET /api/v1/user/info ──透传真实云─→ 响应改写：注入 is_member/memberExpiredTime
   │                                       (真账号 user_id 保留，消除 user_id_mismatch 重试风暴)
   └─ 其余(登录票据/设备列表/H5/sentry) ──透传真实云─→ 会话/设备/云游戏全部真实可用
```

关键工程点：
- **UDP:53 直解真实 IP**（`dns.Resolver` + 223.5.5.5/114.114.114.114）绕过 hosts；DoH 在本机被墙（dns.google 超时、cloudflare 重置）
- 真实上游：api.nrd.nie.163.com=124.160.141.98，uuyc.webapp.163.com=112.83.164.101（CDN vedcdnlb）
- 透传时 `servername: host` + `rejectUnauthorized:false` 连真实云；`passthru*` 分支缓冲改写 user/info
- 信令域名早期被全量接管导致设备列表空/`-100` 断连 → 改透传后 `GET /cloud-device -> 304` 真实云恢复
- 证书 SAN 覆盖 14 域（含 uuyc/feedback/ugc/adl），regen 后重导入 LocalMachine\Root

## 7. 动态期新发现（访问日志实测）

- 真实请求面比静态全：新增 `/sdk/project_config`、`/api/69|337/envelope/`(sentry DSN 69+337)、
  `/api/v1/user/ticket/for/wvlogin`(WebView 免登票据)、`/api/v1/release/fetch/update`、
  `/api/v1/web/cloud_pc/goods/pc`、`/api/v1/web/cloud_pc/recommend/games/cloud_game`
- 访问日志：`C:\ProgramData\leeconnect-access.log`（SYSTEM 进程写工作区曾权限不通，落 ProgramData）
- 教训：假 `user/info` 与真登录账号冲突 → 每秒 `member_info_query discarded reason=user_id_mismatch`
  重试风暴；响应改写（保 user_id、注 VIP 字段）根治

## 8. 验证清单

- [x] **NET-001 动态验证通过（20:45）**：hosts 接管 + LocalMachine 根证书 + 443 假云，
  真实 GameViewer.exe 重启首条 `GET /api/v1/member/entitlements -> 200 389B`，**零字节云端权益接管**
- [x] mock-cloud 全链 TLS：`--use-system-ca` 真实域名 200（OpenSSL3 走系统根，无 pinning 拦截）
- [x] CFG-001：4 设备 ini 全部 `open=1`（.lee-bak 备份在位）
- [x] **配置面全解锁实锤**：`configures_with_version -> 17634B`（final_version 锁清空、member_privilege=lifetime）
- [x] **user/info 改写流**：`passthru* -> rewritten 451B (VIP injected, real account preserved)`
- [x] **UI 复核（docs/ui-check-3-crop.png）**：红条消失、侧栏完整、无会员拦截弹窗；
  设备列表/会话走真实云透传恢复
- [x] PC-001/PC-002 分支强制：Host 12 处 + Client 12 处 je/jne→NOP（force-branches.js 台账化重放）
- [x] LeeConnect.exe / LeeConnect-Host.exe 品牌化+patch+自签 `Get-AuthenticodeSignature = Valid`
- [x] 开机持久接管：计划任务 `LeeConnectMockCloud`（SYSTEM/AtStartup/失败重启×3）
- [x] **登录后端到端闭环（21:16，截图 docs/ui-logged-in.png）**：
  真账号登录成功 → `passthru POST web/login/by_ticket -> 200`（真实云）+
  `INTERCEPT GET member/entitlements -> 200 389B`（假云 VIP）+
  `passthru* GET user/info -> rewritten 451B (VIP injected, real account preserved)` 持续流；
  UI 设备列表完整回归（DESKTOP-M2SOF0N 在线/工位约/LAPTOP/MacBook，走真实云 `room/join/by_device -> 200`），
  功能入口（文件传输/观看模式/终端/端口映射=能力表 feature）全部渲染，无会员拦截弹窗，err=0
- [ ] 双机联测（LeeConnect 自研信令全接管后）：配对/2K-4K 拉流/输入注入/剪贴板/断线 relay
- [ ] （可选）ResourceHacker 清 RCData 图标/短词残留

## 9. 运行态快照（接管持续中）

| 组件 | 状态 |
|---|---|
| 443 假云 | SYSTEM 计划任务 `LeeConnectMockCloud`（AtStartup + 失败重启×3），重启不丢 |
| 访问日志 | `C:\ProgramData\leeconnect-access.log` |
| 权益数据流 | `INTERCEPT .../configures -> 17634B` + `passthru* .../user/info -> rewritten 451B` |
| 真实账号 | 保留（透传），设备/会话/云游戏走真实云 |
| LeeConnect.exe / -Host.exe | 品牌化+12 处分支强制+自签 Valid，`client/build/` |
| 回滚 | `tools/phase2-revert.ps1`（hosts/证书/INI）+ 各 .orig/.pre-brand/.pre-branch 备份 |
