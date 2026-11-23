'use strict';
/**
 * LeeConnect RE: locate entitlement/verification branch sites by byte-pattern.
 * Targets (found via find-xrefs on runtime_product_identity / _verification):
 *   P1  80 bb f8 00 00 00 00 74    cmp byte [rbx+0xF8],0 ; je  -> skip success path
 *   P2  80 bd f8 00 00 00 00 74    same, rbp base
 *   P3  41 f6 c4 02 74              test r12b,0x02 ; je
 *   P4  40 f6 c7 02 74              test dil,0x02 ; je
 *   P5  f6 c3 02 74 / f6 c3 04 74 / f6 c3 10 74 / f6 c3 01 74   feature-bit tests (bl)
 * Emits exact file offsets + suggested NOP (74 xx -> 90 90 forces fallthrough = success branch).
 * Usage: node tools/find-branch-sites.js <binary> [--json]
 */
const fs = require('fs');
const buf = fs.readFileSync(process.argv[2]);
const asJson = process.argv.includes('--json');

const pats = {
  P1_flag_rbx: '80bbf800000000 74',
  P2_flag_rbp: '80bdf800000000 74',
  P3_r12b: '41f6c402 74',
  P4_dil: '40f6c702 74',
  P5a_bl02: 'f6c302 74',
  P5b_bl04: 'f6c304 74',
  P5c_bl10: 'f6c310 74',
  P5d_bl01: 'f6c301 74',
};

function hexToBuf(h) { return Buffer.from(h.replace(/\s+/g, ''), 'hex'); }
function findSeq(hay, prefixHex, condByte) {
  const pre = hexToBuf(prefixHex.split(' ')[0]);
  const out = [];
  let i = -1;
  while ((i = hay.indexOf(pre, i + 1)) >= 0) {
    const at = i + pre.length;
    if (at < hay.length && hay[at] === condByte) out.push(at);   // offset of the 74 (je)
    if (out.length >= 40) break;
  }
  return out;
}

const report = {};
for (const [name, spec] of Object.entries(pats)) {
  const [pre, cond] = spec.split(' ');
  const hits = findSeq(buf, pre, parseInt(cond, 16));
  report[name] = hits;
  if (!asJson) console.log(`${name.padEnd(12)} -> ${hits.length} je-sites: ` + hits.slice(0, 12).map(o => '0x' + o.toString(16)).join(', '));
}
if (asJson) console.log(JSON.stringify(report));
