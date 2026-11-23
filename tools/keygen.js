'use strict';
/**
 * LeeConnect keygen —— SERIAL 生成/校验命令行（与 server/auth.js 同一 HMAC 规则）。
 * 用法:
 *   node tools/keygen.js gen  --machine <MACHINE_ID> [--secret <SECRET>]
 *   node tools/keygen.js check --machine <MACHINE_ID> --serial <SERIAL>
 * 沙箱 fixture：默认 [SECRET] 占位，正式注入 LEECONNECT_SECRET。
 */
process.env.LEECONNECT_SECRET = process.env.LEECONNECT_SECRET || '[SECRET]';
const { genSerial, verifySerial } = require('../server/auth');

const argv = process.argv.slice(2);
const mode = argv[0] || 'gen';
const opt = (k) => { const i = argv.indexOf(`--${k}`); return i >= 0 ? argv[i + 1] : undefined; };
const machine = opt('machine') || '[MACHINE_ID]';

if (mode === 'gen') {
  console.log(genSerial(machine));
} else if (mode === 'check') {
  const serial = opt('serial') || '';
  console.log(verifySerial(serial, machine) ? 'VALID' : 'INVALID');
} else {
  console.error('usage: keygen.js gen|check --machine <ID> [--serial <S>]');
  process.exit(2);
}
