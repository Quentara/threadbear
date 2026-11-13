// Unit tests may inspect fixture accounts, never the machine's accounts.
// HOME alone is insufficient: a copied environment can still name live
// CODEX_HOME/XDG folders, and a caught filesystem error can hide a leak.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { syncBuiltinESMExports } from 'node:module';

const realHome = os.userInfo().homedir;
const resolve = fs.realpathSync;
const canonicalHome = resolve(realHome);
const codeRoot = resolve(path.join(import.meta.dirname, '../../..'));
const testHome = resolve(fs.mkdtempSync(path.join(os.tmpdir(), 'tb-unit-home-')));
process.env.HOME = testHome;
process.env.USERPROFILE = testHome;
// On check hosts TMPDIR is under the real home. Trust scans walk a
// fixture's ancestors up to HOME, so its temp files must be below HOME too.
const temp = path.join(testHome, 'tmp');
fs.mkdirSync(temp);
for (const key of ['TMPDIR', 'TMP', 'TEMP']) process.env[key] = temp;
for (const [name, suffix] of Object.entries({ XDG_CONFIG_HOME: '.config', XDG_DATA_HOME: '.local/share', XDG_CACHE_HOME: '.cache', XDG_STATE_HOME: '.local/state', CODEX_HOME: '.codex' })) {
  process.env[name] = path.join(testHome, suffix);
}
// A test that needs an override supplies its own; never inherit the live
// runner's data, account folders, config files or thread credentials.
for (const name of Object.keys(process.env)) {
  if (/^THREAD(?:BEAR|BARE)_(?:HOME|DATA_DIR|RUNNER_HOME|CLAUDE_MAIN_DIR|CLAUDE_MAIN_JSON|ACCOUNTS_DIR|CODEX_MAIN_DIR|TOKEN_STATS_LIMITS|THREAD_KEY|THREAD_ID|URL|LOG)$/.test(name) || /^OPENCODE_CONFIG(?:_DIR|_CONTENT)?$/.test(name)) delete process.env[name];
}

const roots = [realHome, canonicalHome].flatMap((home) => ['.claude', '.claude.json', '.codex', '.cursor', '.opencode', '.config/opencode', '.local/share/opencode', 'Library/Application Support/Token Stats', 'Library/Application Support/Threadbear'].map((p) => path.join(home, p)));
const violations = new Set<string>();
function forbidden(file: string): boolean {
  return roots.some((root) => file === root || file.startsWith(root + path.sep));
}
function reject(file: string): never {
  violations.add(file);
  throw new Error(`Unit test reached a real account folder: ${file}`);
}
function canonicalPath(named: string): string {
  // The kernel refuses paths this long. Do not walk thousands of missing
  // ancestors in malformed-path tests just to discover the same failure.
  if (Buffer.byteLength(named) > 4096) return named;
  let ancestor = named;
  while (true) {
    try { return path.join(resolve(ancestor), path.relative(ancestor, named)); } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENAMETOOLONG') return named;
    }
    const parent = path.dirname(ancestor);
    if (parent === ancestor) return named;
    ancestor = parent;
  }
}
// A thread inherits the installed release's bin directory. Probing every
// PATH entry must not probe that release or accidentally invoke its tb.
process.env.PATH = (process.env.PATH ?? '').split(path.delimiter).filter((p) => !p || (!forbidden(path.resolve(p)) && !forbidden(canonicalPath(path.resolve(p))))).join(path.delimiter);

function check(file: unknown): void {
  if (typeof file !== 'string' && !Buffer.isBuffer(file) && !(file instanceof URL)) return;
  const named = path.resolve(file instanceof URL ? fileURLToPath(file) : file.toString());
  // Resolve existing ancestors too, so a fixture symlink cannot lead back
  // into a real login, even when the requested leaf doesn't exist yet.
  // The runner also checks its own update clone, inside its real data
  // dir. Its source and dependencies are fixtures, not live runner data.
  if (forbidden(named) && named !== codeRoot && !named.startsWith(codeRoot + path.sep)) reject(named);
  const canonical = canonicalPath(named);
  if (forbidden(canonical) && canonical !== codeRoot && !canonical.startsWith(codeRoot + path.sep)) reject(named);
}

// Patch both forms: several production callers swallow ENOENT, so the
// exit check below also fails the file after any attempted forbidden read.
for (const target of [fs, fs.promises]) {
  for (const name of ['access', 'exists', 'lstat', 'open', 'openAsBlob', 'opendir', 'readFile', 'readdir', 'readlink', 'realpath', 'stat', 'statfs', 'watch', 'watchFile', 'createReadStream', 'writeFile', 'appendFile', 'mkdir', 'rm', 'rmdir', 'unlink', 'chmod', 'chown', 'utimes', 'truncate', 'createWriteStream', 'copyFile', 'cp', 'rename', 'link', 'symlink']) {
    for (const key of [name, name + 'Sync']) {
      const descriptor = Object.getOwnPropertyDescriptor(target, key);
      if (typeof descriptor?.value !== 'function') continue;
      const original = descriptor.value;
      const guarded = function (this: unknown, file: unknown, ...args: unknown[]) {
        try {
          check(file);
          if (['copyFile', 'cp', 'rename', 'link', 'symlink'].includes(name)) check(args[0]);
        } catch (error) {
          if (target === fs.promises) return Promise.reject(error);
          throw error;
        }
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
