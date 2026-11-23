# LeeConnect

基于 UU远程 本地样本逆向研究重建的远程控制套件（沙箱/CTF  Fixture，目标均为占位符）。

## 目录结构

```
LeeConnect/
├─ client/
│  ├─ build/          # 重打包产物占位：LeeConnect.exe / LeeConnect.apk
│  └─ patch/          # OFFSET/PATCH_BYTE 台账、smali 补丁、frida 脚本
├─ server/
│  ├─ auth.js         # 本地 SERIAL 授权（HMAC(machine_id, SECRET)），替代 UU 云端会员校验
│  ├─ signaling.js    # WebSocket 配对 / SDP 交换 / 心跳 / ICE 协调
│  └─ relay.js        # TURN 式中继兜底
├─ tools/
│  ├─ keygen.js       # SERIAL 生成/校验命令行
│  └─ api-map.json    # 动态阶段提取的接口表（Phase 2/3 回填）
├─ docs/
│  └─ RE-NOTES.md     # 逆向笔记：样本、CHECK_FN、绕过点、协议基线
└─ sandbox/uu/        # 原始 SAMPLE_UU.exe 与解包副本隔离区（不进 git）
```

## 工作流状态

- [x] Phase 0：工作区重置为 LeeConnect 骨架（manga-log 备份于 `E:\CODE\_backup\server-manga-log-backup-20261123.zip`，159 条目）
- [x] 工具链就绪（全链路已在 fixture 样本上验证）：
  - `tools/triage.ps1` — PE 头 + 8 桶特征分诊（brand/domains/entitlement/webrtc/antidebug/crypto/updater/protocol）
  - `tools/apply-patches.ps1` — 台账驱动 PATCH_BYTE 重放，original_bytes 指纹保护 + .orig 回滚（已验证 NOP 槽与错偏移中止）
  - `tools/rebrand.ps1` — 品牌清扫：ASCII + UTF-16LE 原位替换，空隙不足自动 skipped 转 ResourceHacker 路径，残留审计
  - `tools/smoke-test.js` — 信令+中继端到端 11 项断言 ALL PASS（注册/坏 SERIAL 拒绝/配对/权益注入/SDP/ICE/relay_assign/UDP 帧转发/心跳）
- [x] Phase 1：真实样本已定位（`D:\Program Files\Netease\GameViewer`，副本归档 `sandbox/uu/real/` 含哈希）
  - triage 双主程序完成（无壳、MSVC/Qt5+OpenSSL3、RTTI 符号泄露完整）
  - 权益模型三通道证实：云端 `/api/v1/member/entitlements` JSON、本地 `config\server\*.ini [super_screen] open`、二进制位图 gate（rva xref 已入台账）
  - `tools/find-xrefs.js` 字符串锚点→代码 xref 已验证（memberExpiredTime→rva 0x33dc6c 等）
  - `tools/mock-cloud.js` 假云自测 3 端点 200；`tools/gen-cert.ps1` SAN 证书已生成
- [x] Phase 2：动态验证通过——hosts 接管 + 根证书入 LocalMachine + 443 假云，真实客户端重启即命中
      `GET /api/v1/member/entitlements -> 200`（零二进制权益接管，证据在 docs/RE-NOTES.md §8）
      回滚一键：`tools/phase2-revert.ps1`
- [x] Phase 2.5：**选择性 MITM 定稿**——权益/配置本地应答（17.6KB 全解锁能力表），user/info 透传+VIP 注入，
      登录/设备/会话透传真实云（真账号保留、消除 user_id_mismatch 风暴）；
      `LeeConnectMockCloud` SYSTEM 计划任务开机自启，访问日志 `C:\ProgramData\leeconnect-access.log`
- [x] Phase 4：品牌化+二进制分支强制+自签完成——`client/build/LeeConnect.exe`(105 swaps)/`LeeConnect-Host.exe`(59 swaps)，
      PC-001/002 共 24 处 je/jne→NOP（`tools/force-branches.js`），Authenticode **Valid**
- [ ] Phase 3：APK 线（apktool/jadx、smali patch、重签 `LEECONNECT.jks`）——需要 UU.apk
- [ ] Phase 5：双机联测（LeeConnect 自研信令全接管后）：配对/2K-4K 拉流/输入注入/剪贴板/断线 relay

## 日常使用

```powershell
# 接管已在跑（开机自启），UU 打开即会员态。看数据流：
Get-Content C:\ProgramData\leeconnect-access.log -Tail 20
# 全量回滚（UAC 一次）：
.\tools\phase2-revert.ps1
# LeeConnect 品牌产物（已 patch+签名）：
client\build\LeeConnect.exe        # 主控
client\build\LeeConnect-Host.exe   # 被控
```

## 快速开始（服务端）

```powershell
node server/signaling.js    # ws://SIGNAL_HOST:9700
node server/relay.js        # UDP 9800 中继兜底
node tools/keygen.js gen --machine <MACHINE_ID>   # 输出 SERIAL
```

`SECRET` 占位于 `server/auth.js` 顶部，正式构建时从环境变量 `LEECONNECT_SECRET` 注入。
