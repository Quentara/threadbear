// A push held while Mike is at the Mac (convex/push.ts notify, holdPush) goes
// when he leaves (sync:setPresence). A Mac whose lid closed never says he
// left, as its runner sleeps with it, so the push waited for good and the
// phone never buzzed (13 Nov 2026, the implicit state machines scan). The
// cron sends the held pushes of a runner that said he's there but has
// stopped checking in.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { call, load, memoryBackend, THREAD_BASICS } from '../helpers/convex-memory.ts';

const sync = load('sync');
const { PRESENCE_FRESH_MS } = load('lib') as { PRESENCE_FRESH_MS: number };

async function world(lastSeenAgo: number) {
  const b = memoryBackend();
  const now = Date.now();
  await b.ctx.db.insert('runners', { runnerId: 'mac1', name: 'M5', version: '1', lastSeenAt: now - lastSeenAgo, startedAt: new Date().toISOString(), draining: false, threadCount: 2, atMac: true, atMacAt: now - lastSeenAgo });
  await b.ctx.db.insert('threads', { threadId: 'needs', runnerId: 'mac1', ...THREAD_BASICS, title: 'Needs him', view: 'inbox', needsYou: true, pushHeldAt: now - lastSeenAgo });
  await b.ctx.db.insert('threads', { threadId: 'seen', runnerId: 'mac1', ...THREAD_BASICS, title: 'Seen since', view: 'inbox', needsYou: false, pushHeldAt: now - lastSeenAgo });
  return b;
}

const notified = (b: ReturnType<typeof memoryBackend>) => b.scheduled.filter((s: any) => String(s.fn).endsWith('push.notify')).map((s: any) => s.args);

test("a held push goes once its Mac has stopped checking in, as one whose lid closed with Mike at it", async () => {
  const b = await world(PRESENCE_FRESH_MS + 60_000);
  await call(b.ctx, sync.sendStaleHeldPushes, {});
  assert.deepEqual(notified(b), [{ threadId: 'needs', held: true }]);
  assert.ok(b.rows('threads').every((t: any) => t.pushHeldAt === undefined), 'nothing still held');
  assert.equal(b.rows('runners')[0].atMac, true, 'he is there again when it wakes and checks in');
  await call(b.ctx, sync.sendStaleHeldPushes, {});
  assert.equal(notified(b).length, 1, 'once');
});

test('a Mac still checking in with Mike at it keeps its pushes held', async () => {
  const b = await world(30_000);
  await call(b.ctx, sync.sendStaleHeldPushes, {});
  assert.deepEqual(notified(b), []);
  assert.ok(b.rows('threads').every((t: any) => t.pushHeldAt !== undefined));
});
