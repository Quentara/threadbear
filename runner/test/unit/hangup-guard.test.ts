// scripts/hangup-guard.sh: what runs a command on a check host stops it when
// the ssh that started it goes. Here the guard's stdin stands in for the ssh
// channel: closing it is the ssh client dying. The real thing was tried on
// the cloud box and Tinker Desk (Desk Runner) when it was written (13 Nov).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';

const LIB = path.join(import.meta.dirname, '..', '..', '..', 'scripts', 'remote-guard-lib.sh');

function guarded(...args: string[]): string {
  const r = spawnSync('/bin/bash', ['-c', '. "$0"; guarded_cmd "$@"', LIB, ...args], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  return r.stdout;
}

const alive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

async function until(what: string, ok: () => boolean, ms = 10_000) {
  const end = Date.now() + ms;
  while (!ok()) {
    if (Date.now() > end) assert.fail(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

function tmp(t: { after: (fn: () => void) => void }) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tb-guard-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test('a finished command keeps its output and exit code, and gets no stdin', () => {
  const r = spawnSync('/bin/sh', ['-c', guarded('-', 'bash', '-c', `echo "it's out"; echo err >&2; read x; echo "in=[$x]"; exit 7`)], { input: 'typed\n', encoding: 'utf8' });
  assert.equal(r.status, 7);
  assert.equal(r.stdout, "it's out\nin=[]\n");
  assert.equal(r.stderr, 'err\n');
});

test('the ssh going stops the command and everything under it', async (t) => {
  const dir = tmp(t);
  const pids = path.join(dir, 'pids');
  const ssh = spawn('/bin/sh', ['-c', guarded('-', 'bash', '-c', `sleep 300 & echo $! >>"${pids}"; bash -c 'echo $$ >>"${pids}"; sleep 301'`)], { stdio: ['pipe', 'ignore', 'ignore'] });
  t.after(() => ssh.kill('SIGKILL'));
  await until('the command to start', () => fs.existsSync(pids) && fs.readFileSync(pids, 'utf8').trim().split('\n').length === 2);
  const started = fs.readFileSync(pids, 'utf8').trim().split('\n').map(Number);
  assert.ok(started.every(alive));
  ssh.stdin.end();
  await until('the command to stop', () => !started.some(alive));
});

test('a command Desk Runner started away from the guard goes too, by its pidfile', async (t) => {
  const dir = tmp(t);
  const pidfile = path.join(dir, 'run.pid');
  // What remote.sh's run script does there: says where it is, unless the ssh
  // has already gone.
  const away = spawn('/bin/bash', ['-c', `echo $$ >"${pidfile}"; [ ! -e "${pidfile}.gone" ] || exit 129; sleep 300`], { detached: true, stdio: 'ignore' });
  t.after(() => {
    try {
      process.kill(-away.pid!, 'SIGKILL');
    } catch {}
  });
  await until('the run to start', () => fs.existsSync(pidfile));
  const ssh = spawn('/bin/sh', ['-c', guarded(pidfile, 'sleep', '300')], { stdio: ['pipe', 'ignore', 'ignore'] });
  const done = new Promise<void>((r) => ssh.on('exit', () => r()));
  ssh.stdin.end();
  await until('the run to stop', () => away.exitCode !== null || away.signalCode !== null);
  await done;
  assert.ok(!fs.existsSync(pidfile) && !fs.existsSync(`${pidfile}.gone`), 'the guard cleaned up after itself');
});
