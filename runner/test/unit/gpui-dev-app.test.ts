// desktop/scripts/dev-app.sh, the GPUI app's dev-app: what keeps an agent's
// app off Mike's own things and off other threads' apps. Its refusals, whose
// instance a command means, and a whole start, do, stop and idle quit with a
// stand-in for the app (a small node script that speaks the automation
// socket and writes down what it was started with), on the real mock runner,
// in a temp TMPDIR and HOME. The app's own refusal of the real data folder
// and its private pasteboard are Rust's tests (desktop/client/src/config.rs,
// desktop/gpui/src/scripted.rs).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { tmpDir } from '../helpers/fakes.ts';
import { alive, dumpText, pick, refuseHome, since, type Instance } from '../../../desktop/scripts/dev-app.ts';

const DEV_APP = path.join(import.meta.dirname, '..', '..', '..', 'desktop', 'scripts', 'dev-app.sh');

test('start refuses the real data folder, however it is named, unless told', () => {
  const tmp = tmpDir('gpui-dev-app-home-');
  const real = path.join(tmp, 'Threadbear');
  fs.mkdirSync(real);
  fs.symlinkSync(real, path.join(tmp, 'link'));
  assert.match(refuseHome(real, real, false) ?? '', /real data folder/);
  assert.match(refuseHome(path.join(tmp, 'link'), real, false) ?? '', /real data folder/);
  assert.match(refuseHome(`${tmp}/x/../Threadbear`, real, false) ?? '', /real data folder/);
  assert.equal(refuseHome(path.join(tmp, 'dev'), real, false), null);
  assert.equal(refuseHome(real, real, true), null);
});

test('a pid counts as ours only while it is the process we started', () => {
  assert.ok(alive(process.pid, since(process.pid)));
  // The same pid, started at another time: macOS gave it to someone else.
  assert.ok(!alive(process.pid, 'Thu Jan  1 00:00:00 1970'));
  assert.ok(!alive(process.pid, ''));
  assert.ok(!alive(undefined, since(process.pid)));
});

function instance(id: string, thread: string, created: number, checkout = '/repo'): Instance {
  return { id, thread, checkout, created, mock: true, fixtures: [], home: '/tmp/x', live: false, binary: '/bin/app' };
}

test("a command means the caller's newest running instance, never another thread's", () => {
  const all = [instance('theirs', 'other', 30), instance('newer', 'me', 20), instance('older', 'me', 10), instance('elsewhere', 'me', 40, '/other-checkout')];
  const me = { thread: 'me', checkout: '/repo' };
  assert.equal(pick(all, me, undefined, () => true).id, 'newer');
  assert.equal(pick(all, me, undefined, (i) => i.id !== 'newer').id, 'older');
  assert.throws(() => pick(all, me, undefined, (i) => i.id === 'theirs'), /none of yours is running \(theirs: thread other/);
  assert.throws(() => pick([], me, undefined, () => true), /no instance is running/);
  // Named, it's that one.
  assert.equal(pick(all, me, 'theirs', () => true).id, 'theirs');
  assert.throws(() => pick(all, me, 'nobody', () => true), /no instance nobody/);
});

test('the dump reads as a page of text', () => {
  const text = dumpText({
    connection: 'connected',
    selection: { id: 'demo_a', status: 'idle', title: 'A thread' },
    sidebar: { mode: 'inbox', loading: false, sections: [{ name: 'Inbox', threads: [{ id: 'demo_a', status: 'idle', title: 'A thread' }] }] },
    transcript: { threadId: 'demo_a', state: 'loaded', rows: [{ id: 'u:1', kind: 'user', text: 'Hello', onScreen: true }] },
    composer: { draft: 'demo_a', text: '' },
  });
  assert.match(text, /selection: demo_a \(idle\) A thread/);
  assert.match(text, /\* demo_a idle/);
  assert.match(text, /> u:1 user {2}Hello/);
});

// The stand-in app: the automation file, then each command answered. It
// writes down its environment, and quits on `quit`.
const FAKE_APP = `#!/usr/bin/env node
const fs = require('fs'), net = require('net'), path = require('path');
const file = process.env.THREADBEAR_AUTOMATION;
fs.writeFileSync(path.join(path.dirname(file), 'fake-env.json'), JSON.stringify(process.env));
const server = net.createServer((s) => {
  let buf = '';
  s.on('data', (d) => {
    buf += d;
    let at;
    while ((at = buf.indexOf('\\n')) >= 0) {
      const cmd = JSON.parse(buf.slice(0, at));
      buf = buf.slice(at + 1);
      if (cmd.token !== 'tok') { s.write(JSON.stringify({ error: 'wrong token' }) + '\\n'); continue; }
      const dump = { connection: 'connected', sidebar: { mode: 'inbox', loading: false, sections: [] }, transcript: { rows: [] }, composer: { draft: 'new', text: '' }, selection: null };
      const out = cmd.cmd === 'dump' ? { result: dump } : cmd.cmd === 'waitFor' ? { result: 'ok 1 ms' } : cmd.cmd === 'fail' ? { error: 'it failed' } : { result: 'ok' };
      s.write(JSON.stringify(out) + '\\n');
      if (cmd.cmd === 'quit') setTimeout(() => process.exit(0), 50);
    }
  });
});
server.listen(0, '127.0.0.1', () => {
  fs.writeFileSync(file + '.tmp', JSON.stringify({ port: server.address().port, token: 'tok' }));
  fs.renameSync(file + '.tmp', file);
});
`;

function run(env: NodeJS.ProcessEnv, ...args: string[]) {
  const r = spawnSync('bash', [DEV_APP, ...args], { env: { ...process.env, ...env }, encoding: 'utf8', timeout: 60_000 });
  return { status: r.status, out: r.stdout.trim(), err: r.stderr.trim() };
}

test('start, do, dump, stop, and the idle quit, with a stand-in app on the mock', async () => {
  const tmp = tmpDir('gpui-dev-app-');
  const app = path.join(tmp, 'fake-app');
  fs.writeFileSync(app, FAKE_APP, { mode: 0o755 });
  fs.mkdirSync(path.join(tmp, 'tmp'));
  fs.mkdirSync(path.join(tmp, 'home'));
  const env = { TMPDIR: path.join(tmp, 'tmp'), HOME: path.join(tmp, 'home'), TB_GPUI_BIN: app, THREADBEAR_THREAD_ID: 'thread-one', DEV_APP_IDLE_QUIT_SECONDS: '4' };
  const realHome = path.join(tmp, 'home', 'Library', 'Application Support', 'Threadbear');
  fs.mkdirSync(realHome, { recursive: true });
  try {
    // Never the live runner's folder.
    const refused = run(env, 'start', '--home', realHome, '--id', 'r1');
    assert.equal(refused.status, 1);
    assert.match(refused.err, /real data folder/);

    const started = run(env, 'start', '--mock', '--id', 'mine1');
    assert.equal(started.status, 0, started.err);
    assert.equal(started.out, 'mine1');
    const dir = path.join(fs.realpathSync(env.TMPDIR), 'threadbear-gpui-dev-app', 'mine1');
    const seen = JSON.parse(fs.readFileSync(path.join(dir, 'fake-env.json'), 'utf8'));
    // Behind Mike's windows, with its own pasteboard, on the mock's data folder.
    assert.equal(seen.THREADBEAR_KEEP_BEHIND, '1');
    assert.equal(seen.THREADBEAR_PASTEBOARD, 'com.mikerosoft.threadbear.automation.gpui-mine1');
    assert.equal(seen.THREADBEAR_HOME, path.join(dir, 'home'));
    assert.equal(seen.THREADBEAR_AUTOMATION_LIVE, undefined);
    // Never the machine's web runtime, with Mike's Chromium profile.
    assert.equal(seen.THREADBEAR_WEB_RUNTIME_DIR, path.join(fs.realpathSync(env.TMPDIR), 'threadbear-gpui-dev-app', 'web-runtime'));
    assert.ok(fs.existsSync(path.join(dir, 'home', 'secrets', 'runner-token')), "the mock's own token");

    const did = run(env, 'do', '{"cmd":"open","id":"demo_a"}', '{"cmd":"waitFor","ready":true}');
    assert.equal(did.status, 0, did.err);
    assert.deepEqual(did.out.split('\n'), ['open ok', 'waitFor ok 1 ms']);
    const failed = run(env, 'do', '{"cmd":"fail"}', '{"cmd":"open"}');
    assert.equal(failed.status, 1);
    assert.equal(failed.out, 'fail error it failed');
    assert.match(run(env, 'dump').out, /connection: connected/);

    // Another thread's commands don't reach it, and its stop --all leaves it.
    const other = { ...env, THREADBEAR_THREAD_ID: 'thread-two' };
    const theirs = run(other, 'do', '{"cmd":"status"}');
    assert.equal(theirs.status, 1);
    assert.match(theirs.err, /none of yours is running \(mine1: thread thread-o/);
    assert.equal(run(other, 'stop', '--all').out, 'none of yours to stop');
    assert.match(run(other, 'start', '--mock', '--id', 'mine1').err, /someone else's/);
    assert.match(run(env, 'status').out, /mine1 {2}running {2}mock {2}yours/);

    // Quits by itself once nobody's sent a command for a while, mock and all.
    const instance = { ...JSON.parse(fs.readFileSync(path.join(dir, 'instance.json'), 'utf8')), ...JSON.parse(fs.readFileSync(path.join(dir, 'app.json'), 'utf8')) } as Instance;
    assert.ok(instance.supervisorSince && instance.appSince, 'start times recorded');
    const until = Date.now() + 20_000;
    while ((alive(instance.supervisor, instance.supervisorSince) || alive(instance.app, instance.appSince)) && Date.now() < until) await new Promise((r) => setTimeout(r, 200));
    assert.ok(!alive(instance.supervisor, instance.supervisorSince) && !alive(instance.app, instance.appSince), 'quit after the idle time');
    assert.match(fs.readFileSync(path.join(dir, 'supervisor.log'), 'utf8'), /no command for 4 s/);

    // An id a start has taken can't be taken again while it builds.
    const busy = path.join(fs.realpathSync(env.TMPDIR), 'threadbear-gpui-dev-app', 'busy');
    fs.mkdirSync(busy);
    fs.writeFileSync(path.join(busy, 'starting'), JSON.stringify({ pid: process.pid, since: since(process.pid) }));
    assert.match(run(env, 'start', '--mock', '--id', 'busy').err, /being started by another/);
    assert.ok(fs.existsSync(busy), "the other start's folder is left alone");

    // And a stop takes everything away, the mock with it.
    assert.equal(run(env, 'start', '--mock', '--id', 'mine2').status, 0);
    const mockPids = spawnSync('pgrep', ['-f', path.join(fs.realpathSync(env.TMPDIR), 'threadbear-gpui-dev-app', 'mine2', 'home')], { encoding: 'utf8' }).stdout.trim();
    assert.ok(mockPids, 'its mock is running');
    // Even one that won't stop when asked (suspended: SIGTERM waits) is gone once stop says so.
    for (const pid of mockPids.split('\n')) process.kill(Number(pid), 'SIGSTOP');
    assert.equal(run(env, 'stop').out, 'stopped mine2');
    for (const pid of mockPids.split('\n')) assert.equal(since(Number(pid)), '', `mock ${pid} stopped`);
    assert.ok(!fs.existsSync(path.join(fs.realpathSync(env.TMPDIR), 'threadbear-gpui-dev-app', 'mine2')));
  } finally {
    run(env, 'stop', '--everyone');
  }
});
