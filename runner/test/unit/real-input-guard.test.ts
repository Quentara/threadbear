// The checks the real-screen tools make just before each real pointer event
// (features/tools/real-screen.ts), against made-up screens. Nothing here
// opens a window, reads the real screen or posts an event.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { EDGE, guardInput, guardWindowMove, type Screen } from '../../../features/tools/real-screen.ts';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const APP = 100, ALERT = 200, OTHER = 300;
type Win = { pid: number; owner: string; frame: number[] };

/** A 1920 x 1080 display with windows listed front to back. */
function screen(windows: Win[], opts: { recording?: string; displays?: number[][] } = {}): Screen {
  return {
    windowAt: (x, y) => {
      const w = windows.find(({ frame: [fx, fy, fw, fh] }) => x >= fx && x < fx + fw && y >= fy && y < fy + fh);
      return w ? { pid: w.pid, owner: w.owner } : null;
    },
    windowFrame: (pid) => windows.find((w) => w.pid === pid)!.frame,
    displays: () => opts.displays ?? [[0, 0, 1920, 1080]],
    recording: () => opts.recording ?? '',
  };
}
const app: Win = { pid: APP, owner: 'Threadbear', frame: [300, 100, 1240, 820] };

test('a press and drop on the app pass', () => {
  guardInput(APP, [[400, 200], [1000, 600]], screen([app]));
});

test('anything recording stops it before the windows are looked at', () => {
  const s = screen([app], { recording: '123 ffmpeg -f avfoundation' });
  s.windowAt = () => assert.fail('looked at windows while recording');
  assert.throws(() => guardInput(APP, [[400, 200]], s), /recording.*No events were posted/);
});

test('a window that has come up over a later point stops it', () => {
  const alert: Win = { pid: ALERT, owner: 'Crash Reporter', frame: [900, 500, 300, 200] };
  assert.throws(() => guardInput(APP, [[400, 200], [1000, 600]], screen([alert, app])), /Crash Reporter is there/);
});

test('a point off the window, off screen or near an edge is refused', () => {
  const s = screen([app]);
  assert.throws(() => guardInput(APP, [[200, 200]], s), /nothing is there/);
  assert.throws(() => guardInput(APP, [[-10, 500]], s), /off screen/);
  // The window running up to the screen's left edge: a drag that ends on it,
  // but in the Dock's reveal strip, is still refused.
  const edge = screen([{ ...app, frame: [0, 100, 1240, 820] }]);
  assert.throws(() => guardInput(APP, [[EDGE - 1, 500]], edge), /within 24 points/);
  guardInput(APP, [[EDGE, 500]], edge);
  // A second display counts as screen.
  guardInput(APP, [[2100, 500]], screen([{ ...app, frame: [1950, 100, 800, 600] }], { displays: [[0, 0, 1920, 1080], [1920, 0, 1920, 1080]] }));
});

test('a grip drag may end off the window, where the window will be', () => {
  const s = screen([app]);
  // Away by 70 and 50 from the activity bar, then back by the same grip from
  // its new place, which ends left of where the window is then.
  guardWindowMove(APP, [324, 114], [394, 164], s);
  const moved = screen([{ ...app, frame: [370, 150, 1240, 820] }]);
  guardWindowMove(APP, [394, 164], [324, 114], moved);
});

test('a grip drag is refused when the grip is covered or off the window', () => {
  const alert: Win = { pid: ALERT, owner: 'Crash Reporter', frame: [300, 100, 200, 100] };
  assert.throws(() => guardWindowMove(APP, [324, 114], [394, 164], screen([alert, app])), /Crash Reporter is there/);
  const other: Win = { pid: OTHER, owner: 'Finder', frame: [0, 0, 300, 1080] };
  assert.throws(() => guardWindowMove(APP, [200, 114], [270, 164], screen([other, app])), /Finder is there/);
});

test('a grip drag is refused when its drop is over something covering the window now', () => {
  const alert: Win = { pid: ALERT, owner: 'Crash Reporter', frame: [380, 150, 100, 100] };
  assert.throws(() => guardWindowMove(APP, [324, 114], [394, 164], screen([alert, app])), /Crash Reporter is there at|Not in front at \(394, 164\)/);
});

test('a grip drag is refused near an edge or when it would take the window off its display', () => {
  const left = screen([{ ...app, frame: [40, 100, 1240, 820] }]);
  assert.throws(() => guardWindowMove(APP, [64, 114], [10, 114], left), /drop \(10, 114\) is off screen or within 24/);
  const right = screen([{ ...app, frame: [640, 100, 1240, 820] }]);
  assert.throws(() => guardWindowMove(APP, [1740, 114], [1810, 164], right), /off its display/);
  assert.throws(() => guardWindowMove(APP, [324, 114], [394, 164], screen([app], { recording: 'ffmpeg' })), /recording/);
});

// Every real event in these tools has its guard right before it, so a window
// that turns up mid-run, or a drag that moved the app, stops the next event.
test('the drag tools check just before every real event', () => {
  for (const file of ['features/tools/real-gestures.ts', 'features/tools/window-drag.ts']) {
    const lines = fs.readFileSync(path.join(root, file), 'utf8').split('\n');
    const posts = lines.flatMap((line, i) => (/(execFileSync|spawn)\((dragger|gesture)\b/.test(line) ? [i] : []));
    assert.ok(posts.length > 0, `${file} posts no events?`);
    for (const i of posts) {
      const before = lines.slice(Math.max(0, i - 3), i).join('\n');
      assert.match(before, /guard(Input|WindowMove)\(/, `${file}:${i + 1} posts an event without a guard just before it`);
    }
    assert.doesNotMatch(lines.join('\n'), /assertInFront/, `${file} should check with guardInput before each event, not once up front`);
  }
});
