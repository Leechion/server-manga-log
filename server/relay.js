'use strict';
/**
 * LeeConnect 中继兜底（TURN 式）—— NAT 打洞失败时转发媒体/输入帧。
 * 帧格式（占位，待 Phase 2 Wireshark 提取后对齐 UU 私有传输）：
 *   [4B magic 'LEEC'][1B type][2B channel][4B len][payload]
 *   type: 0=video 1=input 2=clipboard 3=file 4=control
 * 纯 stdlib UDP，多路复用按 channel 隔离会话。
 */
const dgram = require('dgram');
const RELAY_HOST = process.env.RELAY_HOST || '0.0.0.0';
const RELAY_PORT = Number(process.env.RELAY_PORT || 9800);

/** channel -> [rinfo, rinfo] 两端注册 */
const sessions = new Map();
const sock = dgram.createSocket('udp4');

sock.on('message', (msg, rinfo) => {
  if (msg.length < 11 || msg.toString('ascii', 0, 4) !== 'LEEC') return;
  const type = msg[4];
  const channel = msg.readUInt16BE(5);
  if (type === 4) { // control: 注册/心跳
    let ep = sessions.get(channel);
    if (!ep) { ep = []; sessions.set(channel, ep); }
    const existing = ep.find(e => e.address === rinfo.address && e.port === rinfo.port);
    if (existing) {
      existing.ts = Date.now();
    } else {
      if (ep.length >= 2) ep.shift();
      ep.push({ ...rinfo, ts: Date.now() });
    }
    return;
  }
  const ep = sessions.get(channel); if (!ep || ep.length < 2) return;
  for (const peer of ep) {
    if (peer.address === rinfo.address && peer.port === rinfo.port) { peer.ts = Date.now(); }
    else { sock.send(msg, peer.port, peer.address); }
  }
});

setInterval(() => { // 60s 空闲回收
  const now = Date.now();
  for (const [ch, ep] of sessions) { if (ep.every(e => now - (e.ts || 0) > 60000)) sessions.delete(ch); }
}, 15000).unref();

sock.bind(RELAY_PORT, RELAY_HOST, () => console.log(`[LeeConnect relay] udp://${RELAY_HOST}:${RELAY_PORT} sessions=${sessions.size}`));
