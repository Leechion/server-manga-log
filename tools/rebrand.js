'use strict';
/**
 * LeeConnect rebrand engine (Node port; PowerShell byte loops are unusable on 74MB samples).
 * In-place equal-or-shorter byte swaps on the build copy; null-terminated aware.
 *   ASCII  needle -> repl (same or shorter length only; longer requires slack -> skipped, listed)
 *   UTF-16LE needle -> repl (same rules)
 * Reports: swap counts + residual brand strings.
 * Usage: node rebrand.js <binary> [--dry] [--old=UU] [--new=LeeConnect] [--proto=uu]
 */
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
const bin = args[0];
if (!bin) { console.error('usage: rebrand.js <binary> [--dry] [--old=UU] [--new=LeeConnect]'); process.exit(2); }
const dry = args.includes('--dry');
const opt = (k, d) => { const a = args.find(x => x.startsWith(`--${k}=`)); return a ? a.split('=')[1] : d; };
const OLD = opt('old', 'UU');
const NEW = opt('new', 'LeeConnect');
const OLD_PROTO = opt('proto', 'uu');

const buf = fs.readFileSync(bin);
const stats = { swaps: 0, skipped: 0, log: [] };

function swap(hay, needle, repl) {
  if (repl.length > needle.length) { stats.skipped++; return false; }   // length-increasing edits => ResourceHacker lane
  let count = 0, i = -1;
  while ((i = hay.indexOf(needle, i + 1)) >= 0) {
    const term = (() => { let t = i + needle.length; while (t < hay.length && hay[t] !== 0) t++; return t; })();
    const tail = term - (i + needle.length);
    if (repl.length + tail < hay.length - i && i + repl.length + tail < hay.length) {
      // shift tail right into slack if needed
      if (repl.length > needle.length) {
        for (let k = tail - 1; k >= 0; k--) hay[i + repl.length + k] = hay[i + needle.length + k];
      }
      repl.copy(hay, i);
      if (repl.length < needle.length) hay[i + repl.length + tail] = 0; // retighten for shortened form handled below
      count++; i += needle.length - 1;
    } else { stats.skipped++; }
  }
  return count;
}

function swapZ(label, oldStr, newStr, enc) {
  const needle = Buffer.from(oldStr, enc);
  const repl = Buffer.from(newStr, enc);
  const before = stats.swaps;
  // count occurrences first
  let i = -1, occ = 0;
  while ((i = buf.indexOf(needle, i + 1)) >= 0) occ++;
  if (!occ) return 0;
  let done = 0;
  if (repl.length === needle.length) {   // equal-length in-place swap keeps string layout intact
    i = -1;
    while ((i = buf.indexOf(needle, i + 1)) >= 0) { repl.copy(buf, i); done++; i += needle.length - 1; }
    stats.swaps += done; stats.log.push(`${label} '${oldStr}'->'${newStr}' x${done}`);
  } else {
    stats.log.push(`${label} '${oldStr}'->'${newStr}' SKIP(len ${needle.length}->${repl.length}, needs RCData/ResourceHacker)`);
  }
  return done;
}

// ASCII + wide sweeps across brand variants
const variants = [...new Set([OLD, OLD.toLowerCase(), OLD.toUpperCase()])];
// same-length brand pairs (equal length => safe in-place). UU(2)/Netease(7) mismatches go to RCData lane.
const PAIRS = [
  ['GameViewer', 'LeeConnect'], ['gameviewer', 'leeconnect'], ['GAMEVIEWER', 'LEECONNECT'],
  ['GameView', 'LeeViewr'], ['UuRemote', 'LeeRemote1'], ['uu_remote', 'leeconnect'], ['uuremote', 'leeconect'],
];
for (const [o, n] of PAIRS) swapZ('a ', o, n, 'latin1');
for (const [o, n] of PAIRS) swapZ('u16', o, n, 'utf16le');
for (const v of variants) swapZ('a ', v, NEW, 'latin1');
for (const v of variants) swapZ('u16', v, NEW, 'utf16le');
// protocol scheme (equal length only without RCData): report hits, no-op when longer
{
  const n = Buffer.from(`${OLD_PROTO}://`, 'latin1');
  let i = -1, c = 0; while ((i = buf.indexOf(n, i + 1)) >= 0) c++;
  if (c) stats.log.push(`scheme '${OLD_PROTO}://' x${c} -> needs RCData/short-name slot (report only)`);
}

// residual audit
const latin = buf.toString('latin1');
const strings = [...new Set(latin.match(/[\x20-\x7e]{6,}/g) || [])];
const residual = strings.filter(s => new RegExp(OLD, 'i').test(s)).slice(0, 40);

console.log(`[rebrand] ${path.basename(bin)} size=${(buf.length/(1024*1024)).toFixed(1)}MB swaps=${stats.swaps} skipped=${stats.skipped}`);
stats.log.forEach(l => console.log('   ' + l));
console.log(`[rebrand] residual brand strings: ${residual.length} (top 12):`);
residual.slice(0, 12).forEach(r => console.log('   | ' + r.slice(0, 90)));

if (dry) { console.log('[rebrand] DRY-RUN, file untouched'); process.exit(0); }
if (stats.swaps > 0) {
  const bak = bin + '.pre-brand';
  if (!fs.existsSync(bak)) fs.copyFileSync(bin, bak);
  fs.writeFileSync(bin, buf);
  console.log(`[rebrand] WRITTEN (${stats.swaps} swaps), backup -> ${path.basename(bak)}`);
} else console.log('[rebrand] 0 swaps, file untouched');
