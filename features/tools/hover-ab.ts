// Two builds of the GPUI app side by side on hover-perf.ts: A, B, A, B, ...,
// each in a fresh app, so both see the same machine as it drifts. Prints each
// measurement's median over the runs for A and B, and B as a share of A.
//
//   node features/tools/hover-ab.ts <A binary> <B binary> [pairs=2]
//
// On Tinker Desk's desktop, inside the perf lock (other perf runs measure there
// too): /usr/bin/lockf ~/.cache/threadbear-perf.lock node features/tools/hover-ab.ts ...
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const [a, b, pairsArg] = process.argv.slice(2);
const pairs = Number(pairsArg ?? 2);
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hover-ab-'));
type R = { n: number; p50: number; p90: number; max: number; queueP50: number; frames: number; cpuPerEvent: number; workMs?: { p50: number; p90: number } };
const runs: Record<'A' | 'B', Record<string, R>[]> = { A: [], B: [] };
for (let i = 0; i < pairs; i += 1) {
  for (const [name, bin] of [['A', a], ['B', b]] as const) {
    const out = path.join(dir, `${name}-${i}.json`);
    console.log(`\n== ${name} run ${i + 1} (${bin})`);
    execFileSync(process.execPath, [path.join(here, 'hover-perf.ts')], { stdio: 'inherit', env: { ...process.env, TB_GPUI_BIN: bin, HOVER_PERF_JSON: out } });
    runs[name].push(JSON.parse(fs.readFileSync(out, 'utf8')));
  }
}
const median = (xs: number[]) => {
  const s = xs.filter((x) => Number.isFinite(x)).sort((p, q) => p - q);
  return s.length ? s[Math.floor(s.length / 2)] : NaN;
};
const r1 = (v: number) => Math.round(v * 10) / 10;
console.log('\nmedian over runs; event to painted frame (ms), and frame work (ms)');
console.log('measurement | A cpu/event | B cpu/event | B/A | A p50 | B p50 | B/A | A p90 | B p90 | B/A | A work p90 | B work p90');
const summary: Record<string, unknown> = {};
for (const key of Object.keys(runs.A[0])) {
  const m = (who: 'A' | 'B', f: (r: R) => number | undefined) => median(runs[who].map((r) => (r[key] ? f(r[key]) ?? NaN : NaN)));
  const row = {
    aCpu: m('A', (r) => r.cpuPerEvent), bCpu: m('B', (r) => r.cpuPerEvent),
    aP50: m('A', (r) => r.p50), bP50: m('B', (r) => r.p50),
    aP90: m('A', (r) => r.p90), bP90: m('B', (r) => r.p90),
    aWork: m('A', (r) => r.workMs?.p90), bWork: m('B', (r) => r.workMs?.p90),
  };
  summary[key] = row;
  console.log(`${key} | ${row.aCpu} | ${row.bCpu} | ${r1((100 * row.bCpu) / row.aCpu)}% | ${row.aP50} | ${row.bP50} | ${r1((100 * row.bP50) / row.aP50)}% | ${row.aP90} | ${row.bP90} | ${r1((100 * row.bP90) / row.aP90)}% | ${row.aWork} | ${row.bWork}`);
}
fs.writeFileSync(path.join(dir, 'summary.json'), JSON.stringify({ runs, summary }, null, 2));
console.log(`\nall numbers: ${dir}`);
