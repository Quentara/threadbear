// This compiles only the screen guard's decision, with made-up windows and
// injected AX answers. It never opens a window or posts input events.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');

test('the real-screen guard requires AX proof for a pass-through Dock window', { skip: process.platform !== 'darwin' }, () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'tb-screen-guard-'));
  try {
    const main = path.join(folder, 'main.swift');
    fs.writeFileSync(main, fs.readFileSync(path.join(root, 'features/tools/window-at.swift'), 'utf8')
      + '\n' + fs.readFileSync(path.join(root, 'runner/test/fixtures/window-at.swift'), 'utf8'));
    const binary = path.join(folder, 'window-at-test');
    execFileSync('swiftc', ['-D', 'TESTING', main, '-o', binary], { timeout: 60_000 });
    assert.match(execFileSync(binary, { encoding: 'utf8', timeout: 10_000 }), /screen guard checks passed/);
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
});
