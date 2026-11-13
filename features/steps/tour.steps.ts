// The tour, the pages' intros and Finish setting up (features/tour.feature):
// the `tour` command says the step on show as "ok 3/10 Star or put away lit:
// 52,264 313x51", in both apps (Tour.swift's Automation, desktop/gpui/src/tour.rs).
import { Given, Then, When } from '@cucumber/cucumber';
import assert from 'node:assert/strict';
import type { ThreadbearWorld } from '../support/world.ts';

const SAMPLES = ['Fix the flaky login test', 'Plan the weekend', 'Draft the release notes'];

async function tourLine(world: ThreadbearWorld): Promise<string> {
  const out = String(await world.app.do({ cmd: 'tour' })).trim();
  return out.slice(Math.max(0, out.indexOf('ok ')));
}

async function untilTour(world: ThreadbearWorld, what: string, check: (line: string) => boolean): Promise<string> {
  const deadline = Date.now() + 15_000;
  for (;;) {
    const line = await tourLine(world);
    if (check(line)) return line;
    if (Date.now() > deadline) throw new Error(`Never saw the tour show ${what}: it said ${line}`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

const titled = (title: string) => (line: string) => new RegExp(`^ok \\d+/\\d+ ${title.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}( \\(.*?\\))? lit:`).test(line);

Given("the pages' intros are on", async function (this: ThreadbearWorld) {
  // A scripted app has them off, so its pages aren't covered.
  await this.app.do({ cmd: 'tour', intros: true });
});

When('I go on in the tour', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'tour', do: 'next' });
});

When('I end the tour', async function (this: ThreadbearWorld) {
  await this.app.do({ cmd: 'tour', do: 'end' });
});

When('I switch to the {word} view', async function (this: ThreadbearWorld, view: string) {
  await this.app.do({ cmd: 'mode', mode: view.toLowerCase() });
});

Then('the tour shows {string}', async function (this: ThreadbearWorld, title: string) {
  await untilTour(this, `"${title}"`, titled(title));
});

Then('the tour shows {string} with a part of the window lit', async function (this: ThreadbearWorld, title: string) {
  const line = await untilTour(this, `"${title}" lighting something`, (l) => titled(title)(l) && !l.endsWith('lit: nothing'));
  assert.match(line, /lit: -?\d+,-?\d+ \d+x\d+$/);
});

Then('the tour shows {string} as a single card', async function (this: ThreadbearWorld, title: string) {
  await untilTour(this, `"${title}" alone`, (l) => l.startsWith(`ok 1/1 ${title} lit:`) && !l.endsWith('lit: nothing'));
});

Then('the tour is gone', async function (this: ThreadbearWorld) {
  await untilTour(this, 'nothing', (l) => l === 'ok off');
});

Then('no tour card shows', async function (this: ThreadbearWorld) {
  await new Promise((r) => setTimeout(r, 800));
  assert.equal(await tourLine(this), 'ok off');
});

Then('the sample threads {string}, {string} and {string} are listed', async function (this: ThreadbearWorld, a: string, b: string, c: string) {
  await this.until('the samples listed', (d) => [a, b, c].every((t) => this.thread(d, t)));
});

Then('no sample thread is listed', async function (this: ThreadbearWorld) {
  await this.until('the samples gone', (d) => !SAMPLES.some((t) => this.thread(d, t)));
});

Then('{string} is drawn hovered, with its buttons out', async function (this: ThreadbearWorld, title: string) {
  await this.until(`"${title}" hovered`, (d) => (this.thread(d, title) as unknown as { hovered?: boolean })?.hovered === true);
});

Then('Finish setting up is open with {string} still to do', async function (this: ThreadbearWorld, title: string) {
  type Setup = { shown: boolean; items: { title: string; done: boolean }[] };
  await this.until(`Finish setting up open with "${title}" to do`, (d) => {
    const setup = (d as unknown as { setup?: Setup }).setup;
    return setup?.shown && setup.items.some((i) => i.title === title && !i.done);
  });
});
