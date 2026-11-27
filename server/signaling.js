'use strict';
/**
 * LeeConnect 信令服务 —— 基于 Phase 2 提取的 UU 配对/协商语义重写。
 * 报文语义（占位，待 RE-NOTES.md 抓包回填精确字段）：
 *   hello/pair → device pairing code（6 位码 + DEVICE_ID）
 *   offer/answer → SDP 交换（类 WebRTC）
 *   ice → candidate 转发
 *   heartbeat → 15s 保活，断线清理
 *   relay_assign → NAT 失败时分配 relay.js 端点
 * 依赖: npm i ws
 */
const http = require('http');
const { WebSocketServer } = require('ws');
const { verifySerial, entitlements, machineId } = require('./auth');

const SIGNAL_HOST = process.env.SIGNAL_HOST || '0.0.0.0';
const SIGNAL_PORT = Number(process.env.SIGNAL_PORT || 9700);

/** device table: device_id -> { ws, name, serial, last_seen } */
const devices = new Map();
/** pairing code -> controller device_id（6 位码 5 分钟有效） */
const pairs = new Map();

function send(ws, type, body) { ws.send(JSON.stringify({ type, ...body, ts: Date.now() })); }

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ app: 'LeeConnect-signaling', devices: devices.size }));
});
const wss = new WebSocketServer({ server });

wss.on('connection', (ws, req) => {
  let dev = null;

  ws.on('message', (raw) => {
    let m; try { m = JSON.parse(raw); } catch { return send(ws, 'error', { msg: 'bad_json' }); }
    switch (m.type) {
      case 'register': { // 被控端上线：DEVICE_ID + SERIAL 本地校验
        if (!verifySerial(m.serial, m.device_id)) return send(ws, 'auth_fail', {});
        dev = m.device_id;
        devices.set(dev, {
          ws,
          name: m.name || 'unnamed',
          serial: m.serial,
          account: m.account || 'default',
          last_seen: Date.now(),
          endpoint: m.endpoint || null,
          public_endpoint: m.public_endpoint || null,
          ca: m.ca || null,
        });
        return send(ws, 'registered', { device_id: dev });
      }
      case 'device_list': { // 查询同账号下的在线被控端列表
        const acct = m.account || 'default';
        const list = [];
        for (const [id, d] of devices) {
          if (d.ws.readyState === 1 && d.endpoint && d.account === acct) {
            list.push({
              device_id: id,
              name: d.name,
              endpoint: d.endpoint,
              public_endpoint: d.public_endpoint || null,
            });
          }
        }
        return send(ws, 'device_list', { devices: list });
      }
      case 'connect_device': { // 免配对码：按 device_id 一键直连
        const t = devices.get(m.target_id);
        if (!t || t.ws.readyState !== 1) return send(ws, 'pair_fail', { reason: 'offline' });
        const initiator = m.controller_id || dev || req.socket.remoteAddress;
        send(t.ws, 'pair_open', { controller: initiator, features: entitlements(t.serial, m.target_id).features });
        ws._peer = m.target_id; t.ws._peer = dev;
        return send(ws, 'pair_ok', { target: m.target_id, endpoint: t.endpoint || null, public_endpoint: t.public_endpoint || null, ca: t.ca || null });
      }
      case 'pair_create': { // 被控端生成配对码
        const code = String(Math.floor(100000 + Math.random() * 900000));
        pairs.set(code, { target: dev, exp: Date.now() + 300000 });
        return send(ws, 'pair_code', { code });
      }
      case 'pair_redeem': { // 控制端凭码绑定
        const p = pairs.get(m.code);
        if (!p || p.exp < Date.now()) return send(ws, 'pair_fail', { reason: 'expired' });
        pairs.delete(m.code);
        p.initiator = m.controller_id || req.socket.remoteAddress;
        const t = devices.get(p.target);
        if (!t || t.ws.readyState !== 1) return send(ws, 'pair_fail', { reason: 'offline' });
        send(t.ws, 'pair_open', { controller: p.initiator, features: entitlements(t.serial, p.target).features });
        ws._peer = p.target; t.ws._peer = dev;
        // hand the controller the host's QUIC endpoint + optional STUN public endpoint + CA root
        return send(ws, 'pair_ok', { target: p.target, endpoint: t.endpoint || null, public_endpoint: t.public_endpoint || null, ca: t.ca || null });
      }
      case 'offer': case 'answer': case 'ice': { // SDP/ICE 透传给对端
        const t = devices.get(ws._peer);
        if (t && t.ws.readyState === 1) return send(t.ws, m.type, { from: dev || ws._peer, sdp: m.sdp || null, candidate: m.candidate || null });
        return send(ws, 'error', { msg: 'no_peer' });
      }
      case 'heartbeat': {
        const d = devices.get(dev || ws._peer); if (d) d.last_seen = Date.now();
        return send(ws, 'heartbeat_ack', {});
      }
      case 'relay_assign': { // 打洞失败 → 分配中继
        const t = devices.get(ws._peer); if (!t) return send(ws, 'error', { msg: 'no_peer' });
        const reqHost = (req.headers.host || '').split(':')[0];
        const relayHost = process.env.RELAY_HOST || (SIGNAL_HOST !== '0.0.0.0' ? SIGNAL_HOST : reqHost);
        const c = { host: relayHost, port: Number(process.env.RELAY_PORT || 9800), channel: Math.random().toString(36).slice(2, 10) };
        send(t.ws, 'relay_assign', c); return send(ws, 'relay_assign', c);
      }
      default: return send(ws, 'error', { msg: 'unknown_type', type: m.type });
    }
  });

  ws.on('close', () => { if (dev) devices.delete(dev); });
});

setInterval(() => { // 心跳超时清理
  const now = Date.now();
  for (const [id, d] of devices) if (now - d.last_seen > 45000) { d.ws.close(); devices.delete(id); }
  for (const [c, p] of pairs) if (p.exp < now) pairs.delete(c);
}, 10000).unref();

server.listen(SIGNAL_PORT, SIGNAL_HOST, () =>
  console.log(`[LeeConnect signaling] ws://${SIGNAL_HOST}:${SIGNAL_PORT} (fixture, SECRET=${require('./auth').SECRET.slice(0,4)}…)`));
