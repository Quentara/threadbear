// How the window looks (features/look.feature): the typefaces, from the
// dump's `look`.
import { Then } from '@cucumber/cucumber';
import type { ThreadbearWorld } from '../support/world.ts';

/** The dump's `look` (desktop/gpui/src/style.rs, `look_dump`). */
type Look = {
  font: string;
  monoFont: string;
  /** Whether Geist and Geist Mono are installed on this machine. */
  geist: boolean;
  geistMono: boolean;
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
