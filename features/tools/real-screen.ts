// For the tools that use the real screen and pointer, which run only on a
// machine nobody's using (Tinker Desk's desktop, AGENTS.md): where a window is
// on screen, and the Swift helpers in app/Tools that post real events.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const work = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'tb-real-screen-'));
process.on('exit', () => fs.rmSync(work, { recursive: true, force: true }));

/** app/Tools/<name>.swift, compiled once into a temp folder. */
export function swiftTool(name: string): string {
  const out = path.join(work, name);
  if (!fs.existsSync(out)) execFileSync('swiftc', ['-O', path.join(root, 'app/Tools', `${name}.swift`), '-o', out]);
  return out;
}

const framer = path.join(work, 'frame');

/** The process's main window: x, y, width and height in screen points from the top left. */
export function windowFrame(pid: number): number[] {
  if (!fs.existsSync(framer)) {
    fs.writeFileSync(`${framer}.swift`, `import CoreGraphics
import Foundation
let pid = Int32(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid && (w[kCGWindowLayer as String] as? Int) == 0 {
  let b = w[kCGWindowBounds as String] as! [String: Double]
  print(Int(b["X"]!), Int(b["Y"]!), Int(b["Width"]!), Int(b["Height"]!))
  break
}
`);
    execFileSync('swiftc', ['-O', `${framer}.swift`, '-o', framer]);
  }
  return execFileSync(framer, [String(pid)]).toString().trim().split(' ').map(Number);
}

const finder = path.join(work, 'window-at');

/** The app whose window is on top at a screen point: what a real click there reaches. */
export function windowAt(x: number, y: number): { pid: number; owner: string } | null {
  if (!fs.existsSync(finder)) {
    execFileSync('swiftc', ['-O', path.join(root, 'features/tools/window-at.swift'), '-o', finder]);
  }
  const out = execFileSync(finder, [String(x), String(y)]).toString().trim();
  if (!out) return null;
  const [pid, ...owner] = out.split(' ');
  return { pid: Number(pid), owner: owner.join(' ') };
}

/**
 * Stops before any real event if the app's window isn't the one on top at
 * every point it's about to press: on 10 Nov another job's windows covered
 * the test window on Tinker Desk, and the drags went to them instead.
 */
export function assertInFront(pid: number, points: [number, number][]): void {
  for (const [x, y] of points) {
    const top = windowAt(x, y);
    if (top?.pid !== pid) {
      throw new Error(`Not in front at (${x}, ${y}): ${top ? top.owner : 'nothing'} is there. No events were posted.`);
    }
  }
}

const displayer = path.join(work, 'displays');

/** Every active display's bounds: x, y, width and height in screen points from the top left. */
export function displays(): number[][] {
  if (!fs.existsSync(displayer)) {
    fs.writeFileSync(`${displayer}.swift`, `import CoreGraphics
var ids = [CGDirectDisplayID](repeating: 0, count: 32)
var count: UInt32 = 0
if CGGetActiveDisplayList(32, &ids, &count) == .success {
  for id in ids.prefix(Int(count)) {
    let b = CGDisplayBounds(id)
    print(Int(b.minX), Int(b.minY), Int(b.width), Int(b.height))
  }
}
`);
    execFileSync('swiftc', ['-O', `${displayer}.swift`, '-o', displayer]);
  }
  return execFileSync(displayer).toString().trim().split('\n').filter(Boolean).map((l) => l.split(' ').map(Number));
}

/** Throws if anything on this machine is recording the screen (AGENTS.md: check `pgrep -fl ffmpeg` first). */
export function recording(): string {
  try {
    return execFileSync('/usr/bin/pgrep', ['-fl', 'ffmpeg'], { encoding: 'utf8' }).trim();
  } catch (error) {
    // pgrep says 1 for "none"; anything else is a failure to look, which mustn't pass as "none".
    if ((error as { status?: number }).status === 1) return '';
    throw error;
  }
}

export type Point = [number, number];
/** What the guards read from the screen, so tests can hand them a made-up one. */
export interface Screen {
  windowAt(x: number, y: number): { pid: number; owner: string } | null;
  windowFrame(pid: number): number[];
  displays(): number[][];
  recording(): string;
}
export const realScreen: Screen = { windowAt, windowFrame, displays, recording };

/**
 * How far in from a display's edge a point an event lands on must be: the
 * Dock overlay exception in window-at refuses the same 24 points, as moving the
 * pointer there can reveal an auto-hidden Dock (or a hot corner) after a check.
 */
export const EDGE = 24;
const inside = ([x, y]: Point, [dx, dy, dw, dh]: number[], inset = 0) => x >= dx + inset && x < dx + dw - inset && y >= dy + inset && y < dy + dh - inset;

/**
 * Checks, immediately before a real event, that it can only reach `pid`:
 * nothing is recording, and `pid`'s window is the one on top at every point
 * the event presses, drops or scrolls at, well in from a display's edge.
 * Throws before anything is posted. Call it again before every event: a
 * window or alert can come up over a point between one drag and the next,
 * and a drag can move the app (10 and 13 Nov).
 */
export function guardInput(pid: number, points: Point[], screen: Screen = realScreen): void {
  const busy = screen.recording();
  if (busy) throw new Error(`Something is recording the screen (${busy.split('\n')[0]}). No events were posted.`);
  const shown = screen.displays();
  for (const p of points) {
    if (!shown.some((d) => inside(p, d, EDGE))) throw new Error(`(${p[0]}, ${p[1]}) is off screen or within ${EDGE} points of its edge. No events were posted.`);
    const top = screen.windowAt(p[0], p[1]);
    if (top?.pid !== pid) throw new Error(`Not in front at (${p[0]}, ${p[1]}): ${top ? top.owner : 'nothing'} is there. No events were posted.`);
  }
}

/**
 * The same for a drag meant to move `pid`'s window by its grip: the press
 * must reach the window, but the drop is where the window will be, so it
 * needn't be the window's now (the drag back to where it was starts and ends
 * outside it). AppKit moves the window with the pointer and keeps the mouse
 * for it all the way, so nothing else is sent the drag. What stays checked:
 * nothing recording, the grip on top and in from the edge, the drop in from
 * every edge (no Dock reveal or hot corner), any part of the drop that's over
 * the window now still the window's (so an alert over it stops it), and the
 * window wholly on a display once moved, so the next grip is on screen too.
 */
export function guardWindowMove(pid: number, from: Point, to: Point, screen: Screen = realScreen): void {
  guardInput(pid, [from], screen);
  const [x, y, w, h] = screen.windowFrame(pid);
  if (!inside(from, [x, y, w, h])) throw new Error(`The grip (${from[0]}, ${from[1]}) isn't on the window. No events were posted.`);
  const shown = screen.displays();
  if (!shown.some((d) => inside(to, d, EDGE))) throw new Error(`The drop (${to[0]}, ${to[1]}) is off screen or within ${EDGE} points of its edge. No events were posted.`);
  const moved = [x + to[0] - from[0], y + to[1] - from[1], w, h];
  const onOne = shown.some(([dx, dy, dw, dh]) => moved[0] >= dx && moved[1] >= dy && moved[0] + w <= dx + dw && moved[1] + h <= dy + dh);
  if (!onOne) throw new Error(`Moving the window to (${moved[0]}, ${moved[1]}) would take it off its display. No events were posted.`);
  if (inside(to, [x, y, w, h])) guardInput(pid, [to], screen);
}

const pointerTool = path.join(work, 'pointer');
function pointerHelper(): string {
  if (!fs.existsSync(pointerTool)) {
    // No arguments: where the pointer is. Two: put it there, with no click.
    fs.writeFileSync(`${pointerTool}.swift`, `import CoreGraphics
let a = CommandLine.arguments
if a.count == 3 {
  CGWarpMouseCursorPosition(CGPoint(x: Double(a[1])!, y: Double(a[2])!))
} else {
  let p = CGEvent(source: nil)!.location
  print(Int(p.x.rounded()), Int(p.y.rounded()))
}
`);
    execFileSync('swiftc', ['-O', `${pointerTool}.swift`, '-o', pointerTool]);
  }
  return pointerTool;
}
/** Where the pointer is now, in screen points from the top left, to put it back after a run. */
export function pointer(): Point {
  return execFileSync(pointerHelper()).toString().trim().split(' ').map(Number) as Point;
}
/** Puts the pointer back where it was, without a click. */
export function movePointer([x, y]: Point): void {
  execFileSync(pointerHelper(), [String(x), String(y)]);
}
