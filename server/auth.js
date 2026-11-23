'use strict';
/**
 * LeeConnect 本地授权层 —— 替代 UU 云端会员校验（CHECK_FN 的自研替身）。
 * SERIAL = base32(HMAC-SHA256(SECRET, machine_id + "|" + feature_pack))[0..24]
 * 沙箱 fixture：SECRET/MACHINE 均为占位符，正式构建注入 LEECONNECT_SECRET。
 */
const crypto = require('crypto');

const SECRET = process.env.LEECONNECT_SECRET || '[SECRET]';
const FEATURES = {
  resolution_2k: true,
  fps_144: true,
  audio: true,
  privacy_screen: true,
  gamepad: true,
  clipboard_sync: true,
  file_transfer: true,
};

function machineId() {
  // Windows 机器码占位：注册表 MachineGuid + volume serial，等价 UU 的 DEVICE_ID 绑定
  return process.env.LEECONNECT_MACHINE || '[MACHINE_ID]';
}

function base32(buf) {
  const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  let bits = 0, value = 0, out = '';
  for (const b of buf) {
    value = (value << 8) | b; bits += 8;
    while (bits >= 5) { out += A[(value >>> (bits - 5)) & 31]; bits -= 5; }
  }
  if (bits) out += A[(value << (5 - bits)) & 31];
  return out;
}

function genSerial(mid, pack = 'full') {
  const h = crypto.createHmac('sha256', SECRET).update(`${mid}|${pack}`).digest();
  return base32(h).slice(0, 20).replace(/(.{5})/g, '$1-').slice(0, 24).replace(/-$/, '');
}

function verifySerial(serial, mid) {
  const expect = genSerial(mid);
  return crypto.timingSafeEqual(
    Buffer.from(serial.padEnd(32)), Buffer.from(expect.padEnd(32))
  );
}

function entitlements(serial, mid) {
  if (!verifySerial(serial, mid)) return { ok: false, features: {} };
  return { ok: true, features: FEATURES };
}

module.exports = { SECRET, FEATURES, machineId, genSerial, verifySerial, entitlements };

if (require.main === module) {
  const mid = machineId();
  const s = genSerial(mid);
  console.log(JSON.stringify({ machine_id: mid, serial: s, verify: verifySerial(s, mid), features: FEATURES }, null, 2));
}
