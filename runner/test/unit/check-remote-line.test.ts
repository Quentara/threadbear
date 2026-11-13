// scripts/check-remote.sh waiting in line (helpers/check-remote-harness.ts
// says how the fake hosts work). On 13 Nov a landing's quick check found both
// hosts busy, took the first (Tinker Desk) and waited there over half an hour
// behind GPUI release builds, while the cloud box, which could run it, was
// free soon after. The whole landing queue waited with it. Now a check
// waiting in line keeps looking at the other hosts, and leaves the line for
// one that's free, but only while its own check hasn't started there.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { linuxDf, GiB, setup } from '../helpers/check-remote-harness.ts';

// Looks every second; MAX_SECS so a run that never leaves a 60 s line fails
// in 20 s rather than hanging the suite.
const ENV = { THREADBEAR_CHECK_POLL_SECS: '1', THREADBEAR_CHECK_MAX_SECS: '20' };
const linux = { os: 'Linux', df: linuxDf(50 * GiB) };

test('waiting in line on a busy host, it moves to another that frees up, and its first copy is withdrawn', () => {
  const s = setup({ h1: { state: 'busy', queue: 60 }, h2: { state: 'busy', freeAfter: 1, ...linux } });
  // A change outside the Swift apps, so the Linux host can take it.
  fs.writeFileSync(path.join(s.repo, 'change.ts'), '\n');
  const r = s.run('h1,h2', 3600, '', ENV);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.waits, /^h1 waiting/m);
  assert.match(r.stdout, /h2 is free now/);
  assert.match(r.stdout, /ran on h2 now/);
  // A new id after a move, never one a withdrawn waiter could take for its own.
  assert.match(r.stdout, /ran on h2 now from .*\/runs\/\d{6}1-\d+-0\/scripts/);
  assert.doesNotMatch(r.stdout, /ran on h1/);
  assert.deepEqual(r.copies, ['h1', 'h2']);
  // Withdrawn: if h1's line ever comes round, there's nothing there to run.
  assert.deepEqual(r.left('h1'), []);
  assert.deepEqual(r.left('h2'), []);
});

test('a withdrawal whose answer is lost is still seen as withdrawn, and the check moves', () => {
  const s = setup({ h1: { state: 'busy', queue: 60, loseAck: true }, h2: { state: 'busy', freeAfter: 1, ...linux } });
  fs.writeFileSync(path.join(s.repo, 'change.ts'), '\n');
  const r = s.run('h1,h2', 3600, '', { ...ENV, THREADBEAR_CHECK_ASK_SECS: '2' });
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /leaving the line on h1/);
  assert.match(r.stdout, /ran on h2 now/);
  assert.doesNotMatch(r.stdout, /ran on h1/);
  assert.deepEqual(r.left('h1'), []);
});

test('a check that has started on its host is never moved, even with another free', () => {
  // h1's check runs for 4 s with its copy moved out of runs/, as a started
  // one is; h2 frees up while it runs.
  const s = setup({ h1: { state: 'busy', slow: 4 }, h2: { state: 'busy', freeAfter: 1, ...linux } });
  fs.writeFileSync(path.join(s.repo, 'change.ts'), '\n');
  const r = s.run('h1,h2', 3600, '', ENV);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /ran on h1 after waiting/);
  assert.doesNotMatch(r.stdout, /ran on h2|is free now/);
  assert.deepEqual(r.copies, ['h1']);
});

test('with one host, it waits in line there as before', () => {
  const s = setup({ h1: { state: 'busy', queue: 3 } });
  const r = s.run('h1', 3600, '', ENV);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /ran on h1 after waiting/);
  assert.deepEqual(r.copies, ['h1']);
});

test("a free host that can't take this check is no reason to leave the line", () => {
  // The change is under ios/, so the Linux host can't run it.
  const s = setup({ h1: { state: 'busy', queue: 3 }, h2: { state: 'free', ...linux } });
  fs.mkdirSync(path.join(s.repo, 'ios'));
  fs.writeFileSync(path.join(s.repo, 'ios', 'x.swift'), '\n');
  const r = s.run('h1,h2', 3600, '', ENV);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /ran on h1 after waiting/);
  assert.deepEqual(r.copies, ['h1']);
});

test('a slow copy stays in line rather than copying again elsewhere', () => {
  const s = setup({ h1: { state: 'busy', queue: 3 }, h2: { state: 'busy', freeAfter: 1, ...linux } });
  fs.writeFileSync(path.join(s.repo, 'change.ts'), '\n');
  const r = s.run('h1,h2', 0, '', ENV);
  assert.equal(r.status, 0, r.stdout + r.stderr);
  assert.match(r.stdout, /ran on h1 after waiting/);
  assert.deepEqual(r.copies, ['h1']);
});
