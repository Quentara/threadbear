// How the window looks (features/look.feature): the typefaces and the
// picture behind the window, from the dump's `look`.
import { Then, When } from '@cucumber/cucumber';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ThreadbearWorld } from '../support/world.ts';

const features = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');

/** The dump's `look` (desktop/gpui/src/style.rs, `look_dump`). */
type Look = {
  font: string;
  monoFont: string;
  /** Whether Geist and Geist Mono are installed on this machine. */
  geist: boolean;
  geistMono: boolean;
  /** "painting", "own" (a picture chosen in Settings), or null before it's loaded. */
  backdrop: string | null;
  panelsSeeThrough: boolean;
};
const look = (d: unknown) => (d as { look?: Look | null }).look;

Then("the words are in Geist, or the system's font where it isn't installed", async function (this: ThreadbearWorld) {
  await this.until('the words in Geist where it is installed', (d) => {
    const l = look(d);
    return l && (l.geist ? l.font === 'Geist' : l.font !== 'Geist');
  });
});

Then("code is in Geist Mono, or the system's monospaced font where it isn't installed", async function (this: ThreadbearWorld) {
  await this.until('code in Geist Mono where it is installed', (d) => {
    const l = look(d);
    return l && (l.geistMono ? /^Geist ?Mono$/.test(l.monoFont) : !/^Geist ?Mono$/.test(l.monoFont));
  });
});

Then('the painting is behind the window', async function (this: ThreadbearWorld) {
  await this.until('the painting behind the window', (d) => look(d)?.backdrop === 'painting');
});

Then('the panels are see-through over it', async function (this: ThreadbearWorld) {
  await this.until('see-through panels', (d) => look(d)?.panelsSeeThrough === true);
});

Then('my picture is behind the window', async function (this: ThreadbearWorld) {
  await this.until('my picture behind the window', (d) => look(d)?.backdrop === 'own');
});

When('I choose {string} as the background', async function (this: ThreadbearWorld, file: string) {
  await this.app.do({ cmd: 'backdrop', path: path.join(features, file) });
});

/** Settings > General > Appearance's Background, from the Settings window's dump. */
const background = (d: unknown) => (d as { settingsWindow?: { background?: string | null } | null }).settingsWindow?.background;

Then('Settings says the background is the painting', async function (this: ThreadbearWorld) {
  await this.until('Background: the painting', (d) => background(d) === 'painting');
});

Then('Settings says the background is my picture', async function (this: ThreadbearWorld) {
  await this.until('Background: my picture', (d) => background(d) === 'own');
});
