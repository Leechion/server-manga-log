'use strict';
/**
 * LeeConnect branch forcer: given xref anchor sites, find the entitlement/verification
 * conditional branch within a short window and force the "granted" path.
 *   je-forms (skip-if-not-entitled)  -> NOP the 2-byte 74 xx   (never skip)
 *   jne-forms (take-if-entitled)     -> eb xx                 (always take)
 * Shapes recognized (from find-xrefs context dumps):
 *   80 bb/bd/be f8 00 00 00 00 74   cmp byte [r+0xF8],0 ; je
 *   41 f6 c4 02 74 / 40 f6 c7 02 74 / f6 c3 01|02|04|10 74    test reg,bit ; je
 *   85 c0 75                        test eax,eax ; jne
 * Usage: node tools/force-branches.js <binary> <siteHex,siteHex,...> [--apply]
 */
const fs = require('fs');
const bin = process.argv[2];
const sites = (process.argv[3] || '').split(',').filter(Boolean).map(s => parseInt(s.replace(/^0x/i, ''), 16));
const apply = process.argv.includes('--apply');
if (!bin || !sites.length) { console.error('usage: force-branches.js <binary> <off,...> [--apply]'); process.exit(2); }

const buf = fs.readFileSync(bin);
const HEX = (b, n = 8) => '0x' + b.toString(16).padStart(n, '0');

const SHAPES = [
  { name: 'cmp_flag_je',  pat: Buffer.from('80bbf80000000000', 'hex').slice(0, 7), cond: 0x74, act: 'nop' },
  { name: 'cmp_flag_je2', pat: Buffer.from('80bdf800000000', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'cmp_flag_je3', pat: Buffer.from('80bef800000000', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_r12b_je', pat: Buffer.from('41f6c402', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_dil_je',  pat: Buffer.from('40f6c702', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_bl01_je', pat: Buffer.from('f6c301', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_bl02_je', pat: Buffer.from('f6c302', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_bl04_je', pat: Buffer.from('f6c304', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_bl10_je', pat: Buffer.from('f6c310', 'hex'), cond: 0x74, act: 'nop' },
  { name: 'test_eax_jne', pat: Buffer.from('85c0', 'hex'), cond: 0x75, act: 'jmp' },
];

const plan = [];
for (const site of sites) {
  let best = null;
  for (const sh of SHAPES) {
    for (let k = site; k <= site + 40 && k + sh.pat.length + 1 < buf.length; k++) {
      if (buf.slice(k, k + sh.pat.length).equals(sh.pat)) {
        const condAt = k + sh.pat.length;
        if (buf[condAt] === sh.cond) { best = { sh, condAt }; break; }
      }
    }
    if (best) break;
  }
  if (!best) { console.log(`site ${HEX(site)}: no recognized branch in +40 window (skip)`); continue; }
  const { sh, condAt } = best;
  const before = buf.slice(condAt, condAt + 2);
  const after = sh.act === 'nop' ? Buffer.from([0x90, 0x90]) : Buffer.from([0xeb, before[1]]);
  plan.push({ site, condAt, shape: sh.name, before, after });
  console.log(`site ${HEX(site)} -> ${sh.name.padEnd(13)} je/jne @${HEX(condAt)} [${before.toString('hex')}] => [${after.toString('hex')}] (${sh.act})`);
}

if (!apply) { console.log(`DRY-RUN: ${plan.length} sites resolved, file untouched`); process.exit(0); }
if (!plan.length) { console.log('nothing to do'); process.exit(0); }

const bak = bin + '.pre-branch';
if (!fs.existsSync(bak)) fs.copyFileSync(bin, bak);
for (const p of plan) buf[p.condAt] = p.after[0], buf[p.condAt + 1] = p.after[1];
fs.writeFileSync(bin, buf);
// verify
const chk = fs.readFileSync(bin);
let okAll = true;
for (const p of plan) if (chk.slice(p.condAt, p.condAt + 2).toString('hex') !== p.after.toString('hex')) okAll = false;
console.log(`APPLIED ${plan.length} sites, size=${chk.length} (was ${buf.length}), verify=${okAll ? 'ALL OK' : 'MISMATCH'}, backup=${bak.split(/[\\/]/).pop()}`);
