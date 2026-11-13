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

First A/B (13 Nov, two pairs, mock threads still light): too noisy to read
(the same build's median moved from 19 to 68 ms between runs while other
threads' builds ran). Since then the harness reports main-thread CPU, and
the mock's threads are real-sized.

The activity bar and right rail as views of their own (13 Nov evening,
3 interleaved pairs, main with the fix above against main plus it). The
idea: GPUI's hover styles and `request_animation_frame` notify the view that
drew them, and the root view's observers redraw everything. Measured, it made
no difference, so it didn't land:

| Round | CPU per frame, without | with |
|---|---|---|
| Threads list hover, idle | 15.2 ms | 15.2 ms |
| Activity bar hover, idle | 15.3 ms | 15.1 ms |
| Projects list hover, idle | 12.6 ms | 12.6 ms |
| Activity bar hover, busy | 11.7 ms | 10.7 ms |
| Projects list hover, busy | 12.3 ms | 13.8 ms |

Hover to frame on screen, median, idle: 22 to 35 ms on the threads list and
27 to 37 ms on Projects, both builds. A frame still costs 12 to 15 ms of
main-thread CPU on Tinker Desk after the fix, which is where to look next.

### Where a hover frame goes now (main after the fix, Tinker, `sample` during the sweeps)

- Drawing is about 15% of the main thread while the pointer sweeps the list.
  Of each draw: Taffy laying out the window about half, the Threads panel's
  render about a sixth, painting about a fifth.
- Pointer to frame handed over is about 37 ms median at 60 Hz, two refreshes,
  while a frame's main-thread CPU is about 7 ms. So the rest is frame pacing,
  not work. The Mac build draws through gpui-pre (not gpui-fast, which is
  resolved but never compiled), and its `CAMetalDisplayLink`
  (gpui-pre-apple's `display_link.rs`) leaves `preferredFrameLatency` at
  Apple's default of 2. 1 would show each frame a refresh sooner, if every
  frame fits in one refresh (8.3 ms at 120 Hz). Trying it means vendoring
  gpui-pre-apple, and it changes pacing for scrolling too.

### Frame latency 1 against 2 (14 Nov, one build both ways, 3 interleaved pairs, Tinker at 60 Hz)

`preferredFrameLatency` set through a vendored copy of gpui-pre-apple, with
each display link update's `targetPresentationTimestamp` kept so the probe
could time pointer to glass. Neither landed (no gain, and not worth a
vendored crate): the build is on the branch `latency-wip2` of thread
df197695 if it's wanted again. Medians over the runs:

| Round | Pointer to glass, 2 | 1 | Frames late, 2 | 1 |
|---|---|---|---|---|
| Threads list hover, idle | 66.3 ms | 65.5 ms | 29 | 22 |
| Projects list hover, idle | 63.5 ms | 60.7 ms | 9 | 8 |
| Thread press, idle | 76.0 ms | 67.5 ms | 17 | 15 |
| Threads list hover, busy | 64.0 ms | 63.4 ms | 3 | 4 |
| Projects list hover, busy | 60.7 ms | 61.2 ms | 65 | 58 |
| Thread press, busy | 61.3 ms | 66.1 ms | 9 | 9 |

Scrolling (scroll-perf.ts, two runs each): 99th percentile frame work 18.5 to
19.9 ms either way, 8 to 15 refreshes missed in the second round either way.

No difference: with 1, a frame was still due on the glass about two
refreshes (31 ms) after it was handed over, as with 2, so the setting didn't
take as built. What the numbers do give is main's real latency at 60 Hz:
about 64 ms from the pointer moving to the highlight on the glass, made of
about 5 ms before the app handles the move, about 28 ms to the frame being
handed over (waiting for the display link's next update, then about 7 to 15
ms of work), and about 31 ms from there to the glass.

### Swift app against GPUI

`features/tools/hover-glass.ts` times both apps the same way from outside:
a real pointer move onto a thread list row, and ScreenCaptureKit for when the
row's pixels change. Not run yet: Tinker Desk was waiting on macOS setup
after an update (14 Nov). Run it with
`env TB_GPUI_BIN=<release build> node features/tools/hover-glass.ts` through
`desktop/scripts/remote.sh tinker-desk --desk --perf`.
