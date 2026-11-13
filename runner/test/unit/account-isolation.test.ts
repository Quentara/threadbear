import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { loadConfig } from '../../src/config.ts';
import { discoverAccounts } from '../../src/accounts.ts';
import { tmpDir } from '../helpers/fakes.ts';

test('account discovery keeps the HOME and PATH passed to loadConfig', () => {
  const home = tmpDir();
  const bin = path.join(home, '.opencode', 'bin', 'opencode');
  fs.mkdirSync(path.dirname(bin), { recursive: true });
  fs.writeFileSync(bin, '#!/bin/sh\nexit 0\n', { mode: 0o755 });
  fs.mkdirSync(path.join(home, '.claude'));
  fs.writeFileSync(path.join(home, '.claude.json'), JSON.stringify({ oauthAccount: { emailAddress: 'fixture@example.com' } }));
  fs.mkdirSync(path.join(home, '.codex'));
  // The marker is checked for existence only, never opened.
  fs.writeFileSync(path.join(home, '.codex', 'auth.json'), '');
  const cfg = loadConfig({ HOME: home, PATH: '', THREADBEAR_HOME: path.join(home, 'data') });
  assert.equal(cfg.claudeMainDir, path.join(home, '.claude'));
  assert.equal(cfg.codexMainDir, path.join(home, '.codex'));
  assert.equal(cfg.defaults.cwd, home);
  assert.deepEqual(discoverAccounts(cfg).map((a) => [a.id, a.configDir]), [
    ['claude-main', path.join(home, '.claude')],
    ['codex-main', path.join(home, '.codex')],
    ['opencode-main', path.join(home, '.config', 'opencode')],
  ]);
  fs.unlinkSync(bin);
  assert.deepEqual(discoverAccounts(cfg).map((a) => a.id), ['claude-main', 'codex-main'], 'no fallback to the process PATH');
});

const preload = path.join(import.meta.dirname, '../helpers/clean-env.ts');
function child(source: string, env: NodeJS.ProcessEnv = {}) {
  return spawnSync(process.execPath, ['--import', preload, '--input-type=module', '-e', source], { encoding: 'utf8', env: { ...process.env, ...env } });
}

test('unit-test preload replaces inherited home and account overrides', () => {
  const result = child(`
    import assert from 'node:assert/strict';
    import os from 'node:os';
    const home = process.env.HOME;
    assert.notEqual(home, '/outside/home');
    assert.equal(os.homedir(), home);
    for (const key of ['XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME', 'CODEX_HOME']) assert.ok(process.env[key].startsWith(home + '/'), key);
    for (const key of ['THREADBEAR_HOME', 'THREADBARE_HOME', 'THREADBEAR_CLAUDE_MAIN_JSON', 'THREADBEAR_CODEX_MAIN_DIR', 'CLAUDE_CONFIG_DIR', 'OPENCODE_CONFIG']) assert.equal(process.env[key], undefined, key);
  `, { HOME: '/outside/home', XDG_CONFIG_HOME: '/outside/config', CODEX_HOME: '/outside/codex', CLAUDE_CONFIG_DIR: '/outside/claude', THREADBEAR_HOME: '/outside/data', THREADBARE_HOME: '/outside/old', THREADBEAR_CLAUDE_MAIN_JSON: '/outside/claude.json', THREADBEAR_CODEX_MAIN_DIR: '/outside/codex', OPENCODE_CONFIG: '/outside/opencode.json' });
  assert.equal(result.status, 0, result.stderr);
});

test('unit-test guard fails even when a real-account read error is caught', () => {
  for (const operation of [
    `fs.existsSync(file)`,
    `fs.readFileSync(new URL('file://' + file))`,
    `await fs.promises.readFile(file)`,
    `statSync(file)`,
    `fs.realpathSync.native(file)`,
  ]) {
    const result = child(`
      import fs, { statSync } from 'node:fs';
      import os from 'node:os';
      const file = os.userInfo().homedir + '/.codex/config.toml';
      try { ${operation}; } catch {}
    `);
    assert.equal(result.status, 1, operation + ': ' + result.stderr);
    assert.match(result.stderr, /Unit tests accessed real account folders/);
  }
});

test('unit-test guard follows a fixture symlink back to a real account', () => {
  const result = child(`
    import fs from 'node:fs';
    import os from 'node:os';
    const alias = process.env.HOME + '/alias';
    fs.symlinkSync(os.userInfo().homedir, alias);
    try { fs.existsSync(alias + '/.claude.json'); } catch {}
  `);
  assert.equal(result.status, 1, result.stderr);
  assert.match(result.stderr, /Unit tests accessed real account folders/);
});
