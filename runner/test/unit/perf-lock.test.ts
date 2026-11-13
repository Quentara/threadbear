// Real kernel locks and harmless sleeping children, never a build or a window.
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import { test, type TestContext } from 'node:test';

const root = path.resolve(import.meta.dirname, '../../..');
const helper = path.join(root, 'scripts/perf-lock.pl');
const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));
async function until(check: () => boolean, message: string) {
  for (let i = 0; i < 150; i++) {
    if (check()) return;
    await pause(40);
  }
  assert.fail(message);
}
function host(t: TestContext) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'tb-perf-lock-'));
  const env = { ...process.env, HOME: home, THREADBEAR_PERF_LOCK_OWNER: '' };
  const children: (ChildProcess & { done: Promise<number | null> })[] = [];
  t.after(async () => {
    for (const child of children) child.stdin?.end();
    for (const child of children) await child.done;
    fs.rmSync(home, { recursive: true, force: true });
  });
  function run(args: string[], extra: NodeJS.ProcessEnv = {}, deskPid?: string) {
    const child = spawn('perl', [helper, ...(deskPid ? ['--desk', deskPid] : ['-']), ...args], { env: { ...env, ...extra }, stdio: ['pipe', 'pipe', 'pipe'] });
    const result = child as ChildProcess & { output: string; done: Promise<number | null> };
    result.output = '';
    child.stdout!.on('data', (s) => { result.output += s; });
    child.stderr!.on('data', (s) => { result.output += s; });
    result.done = new Promise((r) => child.on('exit', r));
    children.push(result);
    return result;
  }
  return { home, env, run };
}

test('measurements wait, explain why, and keep both locks for their whole run', async (t) => {
  const h = host(t);
  const first = h.run(['sh', '-c', 'echo first; sleep 1']);
  await until(() => first.output.includes('first'), 'first did not start');
  const second = h.run(['sh', '-c', 'echo second']);
  await until(() => second.output.includes('Waiting for'), 'no waiting message');
  assert.ok(!second.output.includes('second'));
  assert.equal(await first.done, 0);
  assert.equal(await second.done, 0);
  assert.match(second.output, /second/);
});

test('a check lock acquired first blocks measurement, and no check can start during it', async (t) => {
  const h = host(t);
  fs.mkdirSync(path.join(h.home, '.cache/threadbear-check'), { recursive: true });
  const lock = path.join(h.home, '.cache/threadbear-check/lock');
  const check = spawn('perl', ['-MFcntl=:flock', '-e', 'open(my $f, ">>", $ARGV[0]) or die $!; flock($f, LOCK_EX); $|=1; print "ready\\n"; sleep 1', lock], { stdio: ['ignore', 'pipe', 'inherit'] });
  let ready = '';
  check.stdout!.on('data', (s) => { ready += s; });
  await until(() => ready.includes('ready'), 'check did not take lock');
  const run = h.run(['sh', '-c', 'echo measuring; sleep 1']);
  await until(() => run.output.includes('Waiting for'), 'measurement did not wait');
  assert.ok(!run.output.includes('measuring'));
  await until(() => run.output.includes('measuring'), 'measurement did not start');
  const probe = spawn('perl', ['-MFcntl=:flock', '-e', 'open(my $f, ">>", $ARGV[0]) or die $!; exit(flock($f, LOCK_EX|LOCK_NB) ? 0 : 75)', lock]);
  assert.equal(await new Promise((r) => probe.on('exit', r)), 75);
  assert.equal(await run.done, 0);
});

test('nested helper calls and node entry points inherit one lock without deadlock', async (t) => {
  const h = host(t);
  const entry = path.join(h.home, 'entry.mjs');
  const api = path.join(root, 'features/tools/perf-lock.ts');
  fs.writeFileSync(entry, `import { ensurePerfLock } from ${JSON.stringify(api)}; await ensurePerfLock(); console.log('nested');`);
  const child = h.run(['perl', helper, '-', process.execPath, entry]);
  assert.equal(await child.done, 0, child.output);
  assert.match(child.output, /nested/);
  assert.doesNotMatch(child.output, /Waiting/);
});

for (const signal of ['SIGTERM', 'SIGKILL'] as const) test(`${signal} retains locks until a TERM-resistant descendant has stopped`, async (t) => {
  const h = host(t);
  const pidfile = path.join(h.home, 'pid');
  const first = h.run(['sh', '-c', 'trap "" TERM; echo $$ > "$1"; echo ready; while :; do sleep 1; done', 'sh', pidfile]);
  await until(() => first.output.includes('ready'), 'child did not start');
  const pid = Number(fs.readFileSync(pidfile, 'utf8'));
  first.kill(signal);
  const second = h.run(['sh', '-c', 'echo next']);
  await until(() => second.output.includes('Waiting'), 'lock released before cleanup');
  assert.ok(!second.output.includes('next'));
  await first.done;
  assert.equal(await second.done, 0, second.output);
  assert.throws(() => process.kill(pid, 0));
});

test('stdin closing cancels a queued measurement before its command starts', async (t) => {
  const h = host(t);
  const first = h.run(['sh', '-c', 'echo ready; sleep 1']);
  await until(() => first.output.includes('ready'), 'first did not start');
  const second = h.run(['sh', '-c', 'echo should-not-run']);
  await until(() => second.output.includes('Waiting'), 'second did not wait');
  second.stdin!.end();
  assert.notEqual(await second.done, 0);
  assert.doesNotMatch(second.output, /should-not-run/);
});

test('direct node tools acquire the locks and preserve a failing exit status', async (t) => {
  const h = host(t);
  const entry = path.join(h.home, 'entry.mjs');
  fs.writeFileSync(entry, `import { ensurePerfLock } from ${JSON.stringify(path.join(root, 'features/tools/perf-lock.ts'))}; await ensurePerfLock(); console.log('entry ran'); process.exit(17);`);
  const child = spawn(process.execPath, [entry], { env: h.env });
  let out = '';
  child.stdout!.on('data', (s) => { out += s; });
  assert.equal(await new Promise((r) => child.on('exit', r)), 17);
  assert.match(out, /entry ran/);
});


test('legacy lockf jobs contend with both helper locks on macOS', { skip: process.platform !== 'darwin' }, async (t) => {
  const h = host(t);
  fs.mkdirSync(path.join(h.home, '.cache/threadbear-check'), { recursive: true });
  const perf = path.join(h.home, '.cache/threadbear-perf.lock');
  const check = path.join(h.home, '.cache/threadbear-check/lock');
  const legacy = spawn('/usr/bin/lockf', ['-k', perf, '/usr/bin/lockf', '-k', check, 'sh', '-c', 'echo legacy; sleep 1']);
  let output = '';
  legacy.stdout!.on('data', (s) => { output += s; });
  await until(() => output.includes('legacy'), 'legacy job did not acquire locks');
  const measured = h.run(['sh', '-c', 'echo measuring; sleep 1']);
  await until(() => measured.output.includes('Waiting'), 'helper did not contend with lockf');
  assert.doesNotMatch(measured.output, /measuring/);
  await until(() => measured.output.includes('measuring'), 'measurement did not start');
  for (const file of [perf, check]) {
    const probe = spawnSync('/usr/bin/lockf', ['-k', '-t', '0', file, 'true']);
    assert.equal(probe.status, 75, file);
  }
  assert.equal(await measured.done, 0);
});

test('remote build wrappers background builds without lowering the benchmark shell', (t) => {
  const h = host(t);
  const bin = path.join(h.home, 'bin');
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, 'cargo'), '#!/bin/sh\necho fake-cargo\n', { mode: 0o755 });
  const run = spawnSync('/bin/bash', ['-c', '. "$1"; cargo build; echo benchmark; test -d "$threadbear_build_bin"', 'bash', path.join(root, 'scripts/build-priority.sh')], {
    env: { ...h.env, TMPDIR: h.home, PATH: `${bin}:${process.env.PATH}` }, encoding: 'utf8',
  });
  assert.equal(run.status, 0, run.stderr);
  assert.match(run.stdout, /fake-cargo\nbenchmark/);
  assert.ok(!fs.readdirSync(h.home).some((n) => n.startsWith('threadbear-build-bin.')));
});

test('real-screen stays a safe import inside a check already holding the host lock', () => {
  const shared = fs.readFileSync(path.join(root, 'features/tools/real-screen.ts'), 'utf8');
  assert.doesNotMatch(shared, /ensurePerfLock/);
  for (const name of ['scroll-perf.ts', 'switch-perf.ts', 'long-thread.ts', 'real-runner.ts', 'real-gestures.ts', 'real-select.ts', 'real-typing.ts', 'real-review-paste.ts']) {
    assert.match(fs.readFileSync(path.join(root, 'features/tools', name), 'utf8'), /await ensurePerfLock\(\)/, name);
  }
});

test('free check probes keep the same inode that waiting measurements have open', () => {
  const check = fs.readFileSync(path.join(root, 'scripts/check-remote.sh'), 'utf8');
  for (const probe of check.matchAll(/\/usr\/bin\/lockf -t 0 ([^;\n]*?)true/g)) {
    assert.match(probe[1], /\b-k\b|^-k\s/, probe[0]);
  }
});

test('Desk Runner receives lock ownership explicitly through a fresh service environment', async (t) => {
  const h = host(t);
  const bin = path.join(h.home, '.local/bin');
  fs.mkdirSync(bin, { recursive: true });
  fs.writeFileSync(path.join(bin, 'desk'), '#!/bin/sh\nexec env -i HOME="$HOME" PATH="$PATH" /bin/bash -c "$1"\n', { mode: 0o755 });
  const entry = path.join(h.home, "desk entry's test.mjs");
  fs.writeFileSync(entry, `import fs from 'node:fs'; import { ensurePerfLock } from ${JSON.stringify(path.join(root, 'features/tools/perf-lock.ts'))}; await ensurePerfLock(); fs.writeFileSync(process.argv[2], String(process.pid)); console.log('desk inherited');`);
  const child = h.run([process.execPath, entry, path.join(h.home, 'desk.pid')], {}, path.join(h.home, 'desk.pid'));
  assert.equal(await child.done, 0, child.output);
  assert.match(child.output, /desk inherited/);
  assert.doesNotMatch(child.output, /Waiting/);
});
