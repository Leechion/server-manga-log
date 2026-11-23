'use strict';
/**
 * LeeConnect 端到端冒烟测试（Phase 5 前置自动化版）。
 * 流程：双端 register → pair_create/redeem → offer/answer/ice 透传
 *       → relay_assign → 真实 UDP 中继帧转发 → 权益表注入检查。
 * 用法: node tools/smoke-test.js  （需先启动 server/signaling.js 与 server/relay.js）
 */
const WebSocket = require('ws');
const dgram = require('dgram');
const crypto = require('crypto');
const { genSerial } = require('../server/auth');

const SIG = process.env.SIG_URL || 'ws://127.0.0.1:9700';
const TARGET_ID = 'fixture-target-' + crypto.randomBytes(3).toString('hex');
const CTRL_ID = 'fixture-ctrl-' + crypto.randomBytes(3).toString('hex');

function mkws() {
  const ws = new WebSocket(SIG);
  const inbox = [];
  ws.on('message', (d) => inbox.push(JSON.parse(d)));
  const wait = (type, ms = 3000) => new Promise((res, rej) => {
    const t0 = Date.now();
    const poll = setInterval(() => {
      const i = inbox.findIndex(m => m.type === type);
      if (i >= 0) { clearInterval(poll); clearTimeout(to); return res(inbox.splice(i, 1)[0]); }
      if (Date.now() - t0 > ms) { clearInterval(poll); rej(new Error('timeout waiting ' + type)); }
    }, 20);
    const to = setTimeout(() => { clearInterval(poll); }, ms + 100);
  });
  return new Promise((res) => ws.on('open', () => res(Object.assign(ws, { wait }))));
}

const results = [];
const ok = (name, cond, detail = '') => { results.push([cond ? 'PASS' : 'FAIL', name, detail]); if (!cond) process.exitCode = 1; };

(async () => {
  const target = await mkws(), ctrl = await mkws();

  // 1. register（本地 SERIAL 授权替代原云校验）
  target.send(JSON.stringify({ type: 'register', device_id: TARGET_ID, serial: genSerial(TARGET_ID), name: 'lee-target' }));
  ctrl.send(JSON.stringify({ type: 'register', device_id: CTRL_ID, serial: genSerial(CTRL_ID), name: 'lee-ctrl' }));
  ok('target registered', (await target.wait('registered')).device_id === TARGET_ID);
  ok('ctrl registered', (await ctrl.wait('registered')).device_id === CTRL_ID);

  // 2. 错误 serial 应被拒
  const bad = await mkws();
  bad.send(JSON.stringify({ type: 'register', device_id: 'x', serial: 'AAAA-BBBB-CCCC-DDDD' }));
  ok('bad serial rejected', (await bad.wait('auth_fail')) !== undefined);
  bad.close();

  // 3. 配对
  const code = (await (async () => { target.send(JSON.stringify({ type: 'pair_create' })); return (await target.wait('pair_code')).code; })());
  ctrl.send(JSON.stringify({ type: 'pair_redeem', code, controller_id: CTRL_ID }));
  ok('pair ok', (await ctrl.wait('pair_ok')).target === TARGET_ID);
  const po = await target.wait('pair_open');
  ok('entitlement features injected', po.features && po.features.resolution_2k === true && po.features.fps_144 === true, JSON.stringify(po.features));

  // 4. SDP/ICE 透传
  ctrl.send(JSON.stringify({ type: 'offer', sdp: 'v=0 fixture-sdp-controller' }));
  ok('offer relayed to target', (await target.wait('offer')).sdp.includes('controller'));
  target.send(JSON.stringify({ type: 'answer', sdp: 'v=0 fixture-sdp-target' }));
  ok('answer relayed to ctrl', (await ctrl.wait('answer')).sdp.includes('target'));
  target.send(JSON.stringify({ type: 'ice', candidate: 'candidate:1 1 udp 127.0.0.1 50001' }));
  ok('ice relayed', (await ctrl.wait('ice')).candidate.includes('50001'));

  // 5. relay_assign 双端分配
  ctrl.send(JSON.stringify({ type: 'relay_assign' }));
  const ra = await ctrl.wait('relay_assign');
  const rb = await target.wait('relay_assign');
  ok('relay assigned symmetric', ra.channel === rb.channel && ra.port === rb.port, `channel=${ra.channel}`);

  // 6. 真实 UDP 中继帧（ctrl 发 → target 收）
  const u1 = dgram.createSocket('udp4'), u2 = dgram.createSocket('udp4');
  await new Promise((r, j) => { u1.once('error', j); u1.bind({ address: '127.0.0.1', port: 0 }, r); });
  await new Promise((r, j) => { u2.once('error', j); u2.bind({ address: '127.0.0.1', port: 0 }, r); });
  const frame = (type, chNum, payload) => {
    const b = Buffer.alloc(11 + payload.length);
    b.write('LEEC', 0, 'ascii'); b[4] = type; b.writeUInt16BE(chNum, 5); b.writeUInt32BE(payload.length, 7); b.write(payload, 11);
    return b;
  };
  const got = new Promise((res) => u2.on('message', (m) => res(m)));
  const chNum = (ra.channel.length * 7) & 0xffff; // 会话号在 fixture 中由 channel 映射
  u1.send(frame(4, chNum, 'reg'), ra.port, '127.0.0.1'); // control 注册
  u2.send(frame(4, chNum, 'reg'), ra.port, '127.0.0.1');
  await new Promise(r => setTimeout(r, 150));
  u1.send(frame(1, chNum, 'mouse:move:1024,768'), ra.port, '127.0.0.1'); // 输入帧 ctrl→target
  const m = await Promise.race([got, new Promise(r => setTimeout(() => r(null), 1500))]);
  ok('UDP relay forwards input frame', !!m && m.toString('ascii', 11) === 'mouse:move:1024,768', m ? m.slice(0, 12).toString('hex') : 'no frame');
  u1.close(); u2.close();

  // 7. 心跳
  target.send(JSON.stringify({ type: 'heartbeat' }));
  ok('heartbeat ack', (await target.wait('heartbeat_ack')) !== undefined);

  target.close(); ctrl.close();
  console.table(results);
  const fails = results.filter(r => r[0] === 'FAIL').length;
  console.log(fails ? `SMOKE: ${results.length - fails}/${results.length} PASS → FAILURES PRESENT` : `SMOKE: ALL ${results.length} PASS`);
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error('SMOKE ERROR:', e.message); process.exit(1); });
