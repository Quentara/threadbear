import { When, Then } from '@cucumber/cucumber';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { sleep } from '../support/driver.ts';
import type { ThreadbearWorld } from '../support/world.ts';

const sample = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../support/sample.png');

async function drop(world: ThreadbearWorld, kind: string, picture: string, vanish = false) {
  await world.app.do({ cmd: 'picture-input', kind, path: picture, drop: true });
  await sleep(60);
  await world.app.dump();
  await world.app.do({ cmd: 'picture-drop', phase: 1 });
  await sleep(60);
  await world.app.dump();
  await world.app.do({ cmd: 'picture-drop', phase: 2, ...(vanish ? { vanish: picture } : {}) });
}

When('I drop a temporary screenshot and its source disappears', async function (this: ThreadbearWorld) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'threadbear-screenshot-'));
  const screenshot = path.join(folder, 'Screenshot.png');
  fs.copyFileSync(sample, screenshot);
  try {
    await drop(this, 'file', screenshot, true);
    for (let i = 0; i < 60 && fs.existsSync(screenshot); i++) { await this.app.dump(); await sleep(50); }
    if (fs.existsSync(screenshot)) throw new Error('The screenshot source was not removed as the drop ended');
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
});

When('I drop a browser image without a file', async function (this: ThreadbearWorld) {
  await drop(this, 'browser', sample);
});

async function pasteHeic(world: ThreadbearWorld, keepFocus = false) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'threadbear-heic-'));
  const picture = path.join(folder, 'photo.heic');
  try {
    execFileSync('/usr/bin/sips', ['-s', 'format', 'heic', sample, '--out', picture], { stdio: 'pipe' });
    await world.app.do({ cmd: 'picture-input', kind: 'heic', path: picture, keepFocus });
  } finally { fs.rmSync(folder, { recursive: true, force: true }); }
}
When('I paste a HEIC picture from the native clipboard', async function (this: ThreadbearWorld) { await pasteHeic(this); });
When('I paste a HEIC picture into the focused answer box', async function (this: ThreadbearWorld) { await pasteHeic(this, true); });

When('I paste a Photos picture beside a normal picture', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'picture-input', kind: 'photos', path: sample });
});

When("I drag a screenshot's floating thumbnail onto the thread", function () {
  throw new Error('Manual: take a screenshot and drag its floating thumbnail into the thread before it disappears');
});

When('I paste a picture with an unavailable file URL', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'picture-input', kind: 'unreadable', path: sample });
});

When('I paste a bundle promising two pictures beside a normal picture', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'picture-input', kind: 'legacy', path: sample });
});
When('I paste a bundle with an unavailable picture and two good pictures beside a normal picture', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'picture-input', kind: 'legacy-error', path: sample });
});
When('I paste a withdrawn promise beside a normal picture', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'picture-input', kind: 'withdrawn', path: sample });
});

When('I paste a promised photo beside a normal picture', async function (this: ThreadbearWorld) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'threadbear-promise-'));
  const binary = path.join(folder, 'picture-source');
  const source = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../tools/picture-promise.m');
  execFileSync('/usr/bin/clang', ['-fobjc-arc', '-framework', 'AppKit', '-framework', 'ApplicationServices', source, '-o', binary], { stdio: 'pipe' });
  // Refuse to post events over someone else's recording.
  try { const recording = execFileSync('/usr/bin/pgrep', ['-fl', 'ffmpeg'], { encoding: 'utf8' }); if (recording.trim()) throw new Error('Tinker Desk is recording; refusing the real drag'); } catch (error) { if ((error as { status?: number }).status !== 1) throw error; }
  const pid = (this.app as unknown as { pid: number }).pid;
  const child = spawn(binary, [sample, String(pid)], { stdio: ['ignore', 'pipe', 'pipe'] });
  let sourceErrors = '';
  let restore: string[] | undefined;
  child.stderr.on('data', (data) => { sourceErrors += data; });
  try {
    const coordinates = await new Promise<string>((resolve, reject) => {
      const lines = createInterface({ input: child.stdout });
      lines.once('line', (name) => { lines.close(); resolve(name); });
      child.once('exit', (code) => reject(new Error(`The file-promise source exited (${code}): ${sourceErrors}`)));
    });
    const points = coordinates.trim().split(/\s+/).map(Number);
    const { assertInFront, windowAt } = await import('../tools/real-screen.ts');
    assertInFront(child.pid!, [[points[0], points[1]]]);
    // A crash notice or another job may cover part of the destination.
    // Use another exposed point inside the thread, never dismiss their UI
    // or post a drop onto whatever happens to be over the original point.
    if (windowAt(points[2], points[3])?.pid !== pid) {
      const left = points[2] - 650, top = points[3] - 300;
      const exposed = [[500, 200], [500, 500], [650, 600], [500, 650]]
        .map(([x, y]) => [left + x, top + y] as [number, number])
        .find(([x, y]) => windowAt(x, y)?.pid === pid);
      if (exposed) { points[2] = exposed[0]; points[3] = exposed[1]; }
    }
    assertInFront(pid, [[points[2], points[3]]]);
    restore = execFileSync(binary, ['--post', ...points.map(String)], { encoding: 'utf8', stdio: 'pipe' }).trim().split(/\s+/);
    try { await this.until('the promised photo and normal picture', (d) => d.composer.images === 2); if (process.env.TB_MEDIA_PROOF) await this.app.shot(process.env.TB_MEDIA_PROOF); } catch (error) { throw new Error(`${error}\n${sourceErrors}`); }
  } finally {
    if (child.exitCode === null && child.signalCode === null) { child.kill('SIGTERM'); await new Promise<void>((resolve) => child.once('exit', () => resolve())); }
    if (restore) { await sleep(200); execFileSync(binary, ['--move', ...restore], { stdio: 'pipe' }); }
    fs.rmSync(folder, { recursive: true, force: true });
  }
});

// A URL served locally supplies real image bytes through the app's HTTP
// client, without depending on an outside website or signed-in browser.
import http from 'node:http';
import { After } from '@cucumber/cucumber';
const servers = new Set<http.Server>();
After(async () => {
  for (const server of servers) {
    server.closeAllConnections();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
  servers.clear();
});
async function browserUrl(world: ThreadbearWorld, delay: number) {
  const server = http.createServer((_, response) => {
    const timer = setTimeout(() => { response.writeHead(200, { 'Content-Type': 'image/png' }); response.end(fs.readFileSync(sample)); }, delay);
    response.on('close', () => clearTimeout(timer));
  });
  servers.add(server);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address() as { port: number };
  await drop(world, 'url', `http://127.0.0.1:${address.port}/photo`);
  if (delay) await world.until('the loading picture chip', (d) => d.composer.text.includes('[Picture 1]'));
}
When('I drop a browser picture offered only as a URL', async function (this: ThreadbearWorld) {
  await browserUrl(this, 0);
});
When('I drop a browser picture whose response is delayed', async function (this: ThreadbearWorld) {
  await browserUrl(this, 10_000);
});

async function ordinaryLink(world: ThreadbearWorld & { fetched?: () => number }, kind: string) {
  let requests = 0;
  const server = http.createServer((_, response) => { requests++; response.end('an ordinary webpage'); });
  servers.add(server);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address() as { port: number };
  world.fetched = () => requests;
  await drop(world, kind, `http://127.0.0.1:${address.port}/one-time-link`);
}
When('I drop an ordinary web link', async function (this: ThreadbearWorld & { fetched?: () => number }) {
  await ordinaryLink(this, 'link');
});
When('I drop a linked picture whose link is a webpage', async function (this: ThreadbearWorld & { fetched?: () => number }) {
  await ordinaryLink(this, 'linked-image');
});
Then('the link has not been fetched', async function (this: ThreadbearWorld & { fetched?: () => number }) {
  await sleep(300);
  assert.equal(this.fetched?.(), 0);
  assert.equal((await this.app.dump()).composer.images, 0);
});
