'use strict';
/**
 * LeeConnect RE: PE string-anchor -> code xref finder.
 * Parses PE section table, maps file offsets <-> RVAs, scans .text for RIP-relative
 * lea/mov ops referencing given string anchors, prints hit sites + context hexdump.
 * Usage: node tools/find-xrefs.js <binary> <needle1> [needle2...]
 */
const fs = require('fs');
const path = process.argv[2];
const needles = process.argv.slice(3);
if (!path || !needles.length) { console.error('usage: find-xrefs.js <binary> <needle...>'); process.exit(2); }

const buf = fs.readFileSync(path);
const e_lfanew = buf.readUInt32LE(0x3C);
if (buf.readUInt32LE(e_lfanew) !== 0x4550) { console.error('not PE'); process.exit(1); }
const nsec = buf.readUInt16LE(e_lfanew + 6);
const optSize = buf.readUInt16LE(e_lfanew + 20);
const imageBase = buf.readBigUInt64LE(e_lfanew + 24 + 0x18); // PE32+
const secOff = e_lfanew + 24 + optSize;
const sections = [];
for (let i = 0; i < nsec; i++) {
  const o = secOff + i * 40;
  sections.push({
    name: buf.toString('ascii', o, o + 8).replace(/\0/g, ''),
    vsize: buf.readUInt32LE(o + 8), vaddr: buf.readUInt32LE(o + 12),
    rawsize: buf.readUInt32LE(o + 16), raw: buf.readUInt32LE(o + 20),
  });
}
const fileToRVA = (off) => { for (const s of sections) if (off >= s.raw && off < s.raw + s.rawsize) return BigInt(s.vaddr + (off - s.raw)); return null; };
const rvaToFile = (rva) => { const v = Number(rva); for (const s of sections) if (v >= s.vaddr && v < s.vaddr + Math.max(s.vsize, s.rawsize)) return s.raw + (v - s.vaddr); return null; };

console.log(`image base: 0x${imageBase.toString(16)} sections: ${sections.map(s => s.name).join(',')}`);

for (const needle of needles) {
  const nb = Buffer.from(needle, 'ascii');
  // find string occurrences within mapped sections
  let si = -1; const anchors = [];
  while ((si = buf.indexOf(nb, si + 1)) >= 0) {
    const rva = fileToRVA(si);
    if (rva === null) continue;
    const sec = sections.find(s => Number(rva) >= s.vaddr && Number(rva) < s.vaddr + s.rawsize);
    if (sec && (sec.name === '.rdata' || sec.name === '.data')) anchors.push({ file: si, rva, sec: sec.name });
  }
  console.log(`\n== anchor "${needle}" -> ${anchors.length} mapped hits (${anchors.slice(0,3).map(a => 'file+0x' + a.file.toString(16) + '@' + a.sec).join(', ')})`);

  // scan .text for RIP-relative instructions referencing anchor RVAs
  const text = sections.find(s => s.name === '.text');
  if (!text || !anchors.length) { console.log('   no .text or no anchors'); continue; }
  const tStart = text.raw, tEnd = text.raw + text.rawsize;
  const anchorSet = new Map(anchors.map(a => [Number(a.rva), a]));
  const found = [];
  const opcodes = [0x8D, 0x8B]; // lea / mov r, [rip+d]
  for (let off = tStart; off < tEnd - 7; off++) {
    // REX.W (0x48/0x4C) + opcode + modrm mod=00 rm=101
    const b0 = buf[off];
    if (!(b0 === 0x48 || b0 === 0x4C)) continue;
    const b1 = buf[off + 1];
    if (!opcodes.includes(b1)) continue;
    const modrm = buf[off + 2];
    if ((modrm >> 6) !== 0 || (modrm & 7) !== 5) continue;
    const disp = buf.readInt32LE(off + 3);
    const insnRVA = Number(fileToRVA(off)); const nextRVA = insnRVA + 7;
    const target = nextRVA + disp;
    const a = anchorSet.get(target);
    if (a) {
      found.push({ code_file: off, code_rva: insnRVA, target: a });
      if (found.length >= 12) break;
    }
  }
  if (!found.length) { console.log('   0 code xrefs (maybe indirect refs or optimized string concat)'); continue; }
  for (const f of found) {
    const ctx = buf.slice(f.code_file, f.code_file + 24);
    console.log(`   xref: file+0x${f.code_file.toString(16)} (rva 0x${f.code_rva.toString(16)}) -> ${f.target.sec}+0x${f.target.rva.toString(16)} [${ctx.toString('hex').match(/../g).join(' ')}]`);
    // backtrace 0x40 bytes to function-ish start (look for int3 padding cc cc or push patterns)
    let fs2 = f.code_file;
    for (let k = f.code_file; k > f.code_file - 0x400 && k > tStart; k--) if (buf[k] === 0xCC && buf[k + 1] === 0xCC) { fs2 = k + 2; break; }
    console.log(`      near func start guess: file+0x${fs2.toString(16)} rva 0x${(fs2 - text.raw + text.vaddr).toString(16)}`);
  }
}
