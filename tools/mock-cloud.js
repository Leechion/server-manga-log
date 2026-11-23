'use strict';
/**
 * LeeConnect 假云（mock business API）—— 替代 api.nrd.nie.163.com。
 * 思路：原版客户端把权益全部走 HTTPS JSON（/api/v1/member/entitlements 等，
 * 解析器 host::LoginManager::queryMemberInfo 消费 member_type/is_member/memberExpiredTime），
 * 因此 hosts 指向 127.0.0.1 + 自签证书即可零二进制点亮所有会员权益。
 * 用法:
 *   1) 生成证书: .\tools\gen-cert.ps1  (产出 sandbox/mockcloud/{key,cert}.pem, 需加入受信任根)
 *   2) hosts 追加: 127.0.0.1 api.nrd.nie.163.com api-ipv4.nrd.nie.163.com api-dcdn.nrd.nie.163.com api-event.nrd.nie.163.com
 *   3) node tools/mock-cloud.js          （监听 443 需管理员或改 8443+Proxifier 重定向）
 * 端点语义从 GameViewer.exe/.GameViewerServer.exe strings 提取（docs/triage-real）。
 */
const fs = require('fs');
const path = require('path');
const https = require('https');

const PORT = Number(process.env.MOCK_PORT || 443);
const CERT_DIR = process.env.MOCK_CERT_DIR || path.join(__dirname, '..', 'sandbox', 'mockcloud');
const ACCESS_LOG = process.env.MOCK_LOG || path.join(process.env.ProgramData || 'C:\\ProgramData', 'leeconnect-access.log');
function log(msg) {
  const line = `[${new Date().toISOString()}] ${msg}`;
  console.log(line);
  try { fs.appendFileSync(ACCESS_LOG, line + '\r\n'); } catch {}
}
const NOW = Date.now();
const FAR_FUTURE = NOW + 10 * 365 * 24 * 3600 * 1000;

// real-shape config table extracted from %LOCALAPPDATA%\GameViewer\cache\config_*.json,
// upgraded: all abilitys final_version cleared (= unlocked), member_privilege => lifetime.
let CONFIGURES = null;
try { CONFIGURES = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'sandbox', 'mockcloud', 'configs.json'), 'utf8')); } catch { CONFIGURES = null; }

const ENTITLEMENTS = {
  code: 0, msg: 'ok',
  data: {
    is_member: true,
    member_type: 3,                    // 最高档
    memberExpiredTime: FAR_FUTURE,
    trial_end: FAR_FUTURE,
    entitlements: {
      super_screen: 1, virtual_screen: 1, enhanced_privacy: 1, screensaver_custom: 1,
      resolution_2k: 1, resolution_4k: 1, fps_144: 1, fps_240: 1,
      audio: 1, gamepad: 1, clipboard_sync: 1, file_transfer: 1,
      second_screen_quality: 'ultra', quality: 'ultra',
    },
  },
};

// device roster harvested from local state (config/server/*.ini device ids + observed UI names)
const DEVICES = [
  { device_id: 'aeawu24b7ia6jrqy', name: '家里电脑', online: true,  type: 1, platform: 8, support_wol: false, version: '4.48.1.5606', bind_time: NOW - 200 * 86400000 },
  { device_id: 'aeawu2cnl4a6dqqk', name: '办公电脑', online: false, type: 1, platform: 8, support_wol: false, version: '4.48.1.5606', bind_time: NOW - 180 * 86400000 },
  { device_id: 'aeawu2cqiqa6drqt', name: '客厅电脑', online: false, type: 1, platform: 8, support_wol: false, version: '4.48.1.5606', bind_time: NOW - 90 * 86400000 },
  { device_id: 'aeawv4oc7ebhs4jk', name: 'my_computer', online: true, type: 1, platform: 8, support_wol: false, version: '4.48.1.5606', bind_time: NOW - 30 * 86400000 },
];
const deviceOk = { code: 0, msg: 'ok', data: { devices: DEVICES, total: DEVICES.length } };

const routes = {
  'GET /api/v1/member/entitlements': ENTITLEMENTS,
  'GET /api/v1/member/trial': ENTITLEMENTS,
  'GET /api/v1/user/info': { code: 0, msg: 'ok', data: { user_id: 10001, nickname: 'LeeOwner', vip_level: 3, member_type: 3, memberExpiredTime: FAR_FUTURE } },
  'GET /api/v1/device/list': deviceOk,
  'POST /api/v1/device/list': deviceOk,
  'GET /api/v1/device/controllable': deviceOk,
  'POST /api/v1/device/controllable': deviceOk,
  'GET /api/v1/device/groups/of/my': deviceOk,
  'POST /api/v1/device/groups/of/my': deviceOk,
  'POST /api/v1/event/report/batch': { code: 0, msg: 'ok' },
  'GET /api/v1/app/global/whitelist': { code: 0, msg: 'ok', data: { list: [] } },
  'POST /api/v1/tool/configures_with_version': CONFIGURES
    ? { code: 0, msg: 'ok', data: CONFIGURES }
    : { code: 0, msg: 'ok', data: { configs: {}, version: 'lee-far-future' } },
  'GET /api/v1/tool/configures_with_version': CONFIGURES
    ? { code: 0, msg: 'ok', data: CONFIGURES }
    : { code: 0, msg: 'ok', data: { configs: {}, version: 'lee-far-future' } },
};

function pickKey() {
  const keyPath = path.join(CERT_DIR, 'key.pem');
  const certPath = path.join(CERT_DIR, 'cert.pem');
  if (fs.existsSync(keyPath) && fs.existsSync(certPath)) {
    return { key: fs.readFileSync(keyPath), cert: fs.readFileSync(certPath) };
  }
  const pfxPath = path.join(CERT_DIR, 'mockcloud.pfx'); // node tls accepts pkcs#12 directly
  if (fs.existsSync(pfxPath)) {
    console.log('[mock-cloud] using PFX credentials (gen-cert.ps1 output)');
    return { pfx: fs.readFileSync(pfxPath), passphrase: process.env.MOCK_PFX_PASS || 'leeconnect' };
  }
  console.error(`[mock-cloud] missing ${keyPath}/${certPath} or ${pfxPath} -> run tools/gen-cert.ps1 first`);
  process.exit(1);
}

const server = https.createServer(pickKey(), (req, res) => {
  const key = `${req.method} ${req.url.split('?')[0]}`;
  const host = req.headers.host || 'api.nrd.nie.163.com';
  // pure-local endpoints: entitlements & configures (shape-served, no real dependency)
  if (/^(GET|POST) \/api\/v1\/(member\/|tool\/configures)/.test(key)) {
    const body = routes[key] || (/entitlement|member/.test(req.url) ? ENTITLEMENTS : { code: 0, msg: 'ok', data: CONFIGURES || {} });
    const json = JSON.stringify(body);
    log(`INTERCEPT ${key} -> 200 ${json.length}B`);
    res.writeHead(200, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(json) });
    return res.end(json);
  }
  // OFFLINE_LOGIN mode (flag file sandbox/mockcloud/offline.flag): fabricate session so the
  // app is usable with no real account. Default OFF -> real login passthru + VIP injection.
  if (offlineMode() && /^(GET|POST) \/api\/v1\/(login\/|qrcode\/|user\/ticket|guest\/)/.test(key)) {
    const t = 'lee-offline-' + crypto.createHash('md5').update(FAR_FUTURE + '').digest('hex').slice(0, 16);
    const body = { code: 0, msg: 'ok', data: { token: t, uu_token: t, user_id: OFFLINE_UID, expire: FAR_FUTURE, login_status: 'success', status: 'scanned' } };
    const json = JSON.stringify(body);
    log(`OFFLINE ${key} -> 200 (fabricated session uid=${OFFLINE_UID})`);
    res.writeHead(200, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(json) });
    return res.end(json);
  }
  if (offlineMode() && /\/api\/v1\/user\/info/.test(req.url)) {
    const body = { code: 0, msg: 'ok', data: { user_id: OFFLINE_UID, nickname: 'LeeOwner(offline)', vip_level: 3, member_type: 3, is_member: true, memberExpiredTime: FAR_FUTURE } };
    const json = JSON.stringify(body);
    log(`OFFLINE ${key} -> 200`);
    res.writeHead(200, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(json) });
    return res.end(json);
  }
  if (offlineMode() && /\/api\/v1\/device\/(list|controllable|groups)/.test(req.url)) {
    const json = JSON.stringify(deviceOk);
    log(`OFFLINE ${key} -> 200 local roster x${DEVICES.length}`);
    res.writeHead(200, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(json) });
    return res.end(json);
  }
  // everything else: passthrough to REAL cloud; user/info gets VIP fields injected in-flight
  passthru(req, res, host, /\/api\/v1\/user\/info/.test(req.url) ? injectVip : null);
});

const crypto = require('crypto');
const OFFLINE_UID = 900000001;
function offlineMode() {
  try { return fs.existsSync(path.join(CERT_DIR, 'offline.flag')); } catch { return false; }
}

function injectVip(json) {
  try {
    const j = JSON.parse(json);
    const d = j.data || j;
    d.is_member = true; d.member_type = 3; d.vip_level = 3;
    d.memberExpiredTime = FAR_FUTURE; d.member_expired_time = FAR_FUTURE;
    log(`  -> user/info VIP fields injected (real account preserved)`);
    return JSON.stringify(j);
  } catch { return json; }
}

const { execFileSync } = require('child_process');
const dns = require('dns');
const realIpCache = new Map();
const resolver = new dns.Resolver();
resolver.setServers(['223.5.5.5', '114.114.114.114']); // UDP:53 DoH is blocked here; plain UDP works and bypasses hosts
function resolveReal(host) {
  return new Promise((resolve) => {
    if (realIpCache.has(host)) return resolve(realIpCache.get(host));
    resolver.resolve4(host, { timeout: 5000 }, (err, addrs) => {
      const ip = !err && addrs && addrs[0] ? addrs[0] : null;
      if (ip) realIpCache.set(host, ip);
      resolve(ip);
    });
  });
}

async function passthru(req, res, host, transform) {
  const ip = await resolveReal(host);
  if (!ip) { log(`passthru ${req.url} -> NO REAL IP (serve empty)`); res.writeHead(200, { 'content-type': 'text/html' }); return res.end('<html><body>leeconnect passthru fallback</body></html>'); }
  const upstream = https.request({
    host: ip, port: 443, path: req.url, method: req.method,
    headers: { ...req.headers, host }, servername: host,
    rejectUnauthorized: false,
  }, (up) => {
    if (!transform) {
      res.writeHead(up.statusCode, up.headers);
      up.pipe(res);
      return log(`passthru ${req.method} ${host}${req.url} -> ${up.statusCode}`);
    }
    const chunks = [];
    up.on('data', c => chunks.push(c));
    up.on('end', () => {
      let out = Buffer.concat(chunks).toString('utf8');
      out = transform(out);
      const h = { ...up.headers };
      delete h['content-encoding']; delete h['transfer-encoding'];
      h['content-length'] = Buffer.byteLength(out);
      res.writeHead(up.statusCode, h);
      res.end(out);
      log(`passthru* ${req.method} ${host}${req.url} -> ${up.statusCode} (rewritten ${out.length}B)`);
    });
  });
  upstream.on('error', (e) => { log(`passthru err ${req.url}: ${e.message}`); res.writeHead(502); res.end(); });
  req.pipe(upstream);
}

server.listen(PORT, '0.0.0.0', () =>
  console.log(`[mock-cloud] listening :${PORT} (fixture cloud for api.nrd.nie.163.com hosts-swap)`));
