// Unit tests may inspect fixture accounts, never the machine's accounts.
// HOME alone is insufficient: a copied environment can still name live
// CODEX_HOME/XDG folders, and a caught filesystem error can hide a leak.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { syncBuiltinESMExports } from 'node:module';

const realHome = os.userInfo().homedir;
const testHome = fs.mkdtempSync(path.join(os.tmpdir(), 'tb-unit-home-'));
process.env.HOME = testHome;
process.env.USERPROFILE = testHome;
for (const [name, suffix] of Object.entries({ XDG_CONFIG_HOME: '.config', XDG_DATA_HOME: '.local/share', XDG_CACHE_HOME: '.cache', XDG_STATE_HOME: '.local/state', CODEX_HOME: '.codex', CLAUDE_CONFIG_DIR: '.claude' })) {
  process.env[name] = path.join(testHome, suffix);
}
// A test that needs an override supplies its own; never inherit the live
// runner's data, account folders, config files or thread credentials.
for (const name of Object.keys(process.env)) {
  if (/^THREAD(?:BEAR|BARE)_(?:HOME|DATA_DIR|RUNNER_HOME|CLAUDE_MAIN_DIR|CLAUDE_MAIN_JSON|ACCOUNTS_DIR|CODEX_MAIN_DIR|TOKEN_STATS_LIMITS|THREAD_KEY|THREAD_ID|URL|LOG)$/.test(name) || /^OPENCODE_CONFIG(?:_DIR|_CONTENT)?$/.test(name)) delete process.env[name];
}

const roots = ['.claude', '.claude.json', '.codex', '.cursor', '.opencode', '.config/opencode', '.local/share/opencode', 'Library/Application Support/Token Stats', 'Library/Application Support/Threadbear'].map((p) => path.join(realHome, p));
const violations = new Set<string>();
const resolve = fs.realpathSync;
function forbidden(file: string): boolean {
  return roots.some((root) => file === root || file.startsWith(root + path.sep));
}
function reject(file: string): never {
  violations.add(file);
  throw new Error(`Unit test reached a real account folder: ${file}`);
}
function check(file: unknown): void {
  if (typeof file !== 'string' && !Buffer.isBuffer(file) && !(file instanceof URL)) return;
  const named = path.resolve(file instanceof URL ? fileURLToPath(file) : file.toString());
  if (forbidden(named)) reject(named);
  // Resolve existing ancestors too, so a fixture symlink cannot lead back
  // into a real login, even when the requested leaf doesn't exist yet.
  let ancestor = named;
  let canonical = named;
  while (true) {
    try { canonical = path.join(resolve(ancestor), path.relative(ancestor, named)); break; } catch { /* Try its parent. */ }
    const parent = path.dirname(ancestor);
    if (parent === ancestor) break;
    ancestor = parent;
  }
  if (forbidden(canonical)) reject(named);
}

// Patch both forms: several production callers swallow ENOENT, so the
// exit check below also fails the file after any attempted forbidden read.
for (const target of [fs, fs.promises]) {
  for (const name of ['access', 'exists', 'lstat', 'open', 'opendir', 'readFile', 'readdir', 'readlink', 'realpath', 'stat', 'watch', 'watchFile', 'createReadStream']) {
    for (const key of [name, name + 'Sync']) {
      const descriptor = Object.getOwnPropertyDescriptor(target, key);
      if (typeof descriptor?.value !== 'function') continue;
      const original = descriptor.value;
      const guarded = function (this: unknown, file: unknown, ...args: unknown[]) {
        check(file);
        return Reflect.apply(original, this, [file, ...args]);
      };
      // fs.realpathSync.native is used to canonicalise paths too.
      if (typeof original.native === 'function') Object.assign(guarded, { native: (file: unknown, ...args: unknown[]) => { check(file); return original.native(file, ...args); } });
      Object.defineProperty(target, key, { ...descriptor, value: guarded });
    }
  }
}
syncBuiltinESMExports();
process.on('exit', () => {
  fs.rmSync(testHome, { recursive: true, force: true });
  if (!violations.size) return;
  console.error('Unit tests accessed real account folders:\n' + [...violations].join('\n'));
  process.exitCode = 1;
});
