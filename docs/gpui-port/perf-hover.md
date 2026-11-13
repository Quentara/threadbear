# Hover and press latency in the GPUI app

Mike, 13 Nov 2026, with a screen recording: "when I scroll up and down, mouse
up and down, there's a noticeable delay on the highlighting in the projects
area. There's just this constant kind of little bit of delay everywhere." The
Swift app doesn't lag like this.

This file keeps what was measured, how, and what each change did. Scrolling
and the transcript (perf 2) and idle work and priority (perf 3) have their own
threads.

## What a hover did

A `sample` of Mike's installed app (1,406 threads, 13 Nov) had the main
thread drawing frames 93% of the time. Half of each frame was the Threads
panel working out its sections: `shown_sections` copied and sorted every
thread, and the list, the Threads button's badge and the arrow keys each
asked for it, three or four times a frame. The side pane copied the whole
thread map twice more as it drew (`schedule_rows`, `started_rows`). And a
row's hover notified the whole app, so it redrew the transcript and the side
pane as well as the list.

## Measuring

- `desktop/core/tests/sidebar_cost.rs`: the sorting and copying on 1,400
  summaries shaped like Mike's (about 3.5 KB each: patches, panels, actions,
  hooks). `cargo test --release -p threadbear-core --test sidebar_cost --
  --ignored --nocapture`.
- `features/tools/hover-perf.ts`: real pointer moves and presses on Tinker
  Desk's screen (`pointer-probe.swift`) down the thread list and the Projects
  list, idle and with every core but one busy. The app's `input-probe`
  (`desktop/gpui/src/input_probe.rs`) gives when each hover was handled,
  every presented frame from GPUI's own journal, and the main thread's CPU
  time. `hover-ab.ts` runs two builds turn about. Wall-clock latency on
  Tinker swings two to three times between runs of the same build while
  other threads build there, so compare main-thread CPU per hover first.
- The mock's big set (`TB_MOCK_PERF_PROJECTS=1400`) now carries real-sized
  summaries: with 0.5 KB ones the copying didn't show.

## Results

### Sorting and copying (sidebar_cost.rs, M5 at background priority, so efficiency cores)

| What | Before | After |
|---|---|---|
| Inbox sections, once | 17.4 ms | same, but only when a thread, the search, the order or a schedule changed |
| Inbox sections, per frame (3 to 4 asks) | 52 to 70 ms | 0 on a hover |
| Side pane's copies of every thread, per draw | 2 x 15.3 ms | 0 |
| Sorting every thread for the Projects list | 1.23 ms | 0.12 ms |

### Real pointer on Tinker Desk

First A/B (13 Nov, two pairs, mock threads still light): too noisy to
read (the same build's median moved from 19 to 68 ms between runs while
other threads' builds ran). Re-run with CPU per hover and real-sized threads
once the perf lock queue is clear: see below.
