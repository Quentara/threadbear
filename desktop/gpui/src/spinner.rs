// The turning arc a working thread shows in the sidebar (Spinner.swift's
// SpinnerView): three quarters of a ring with round ends, once round every
// 0.9 s, clockwise.
//
// The Swift app turns it in the render server, with nothing for the main
// thread to do. GPUI turns a thing by drawing it again, and a request for the
// next frame from inside a view has that view and every view it's in (the
// sidebar, the dock, the window, the open transcript) draw again. The kit's
// spinner asks every frame, so while any thread worked the whole window drew
// at the screen's rate, 120 times a second on Mike's MacBook, and a debug
// build spent all its time there. This one waits 20 ms after it's drawn
// before asking again, one timer for all the arcs a view has: with the wait
// for the screen's next frame on top, about 30 frames a second, which reads
// as smooth at this size. (33 ms came to 20 a second, a visible step.)
//
// Even at that rate a turn of the arc redrew the whole sidebar: about 17 ms a
// frame with a long list, a third of a core while any thread worked (Mike,
// 13 Nov: "everything does seem to slow down a bit once the system loads
// up"). So in a window with a `layer` the arcs aren't drawn where they sit.
// Each notes its place, clip and colour as its row is drawn, and the layer,
// drawn last at the window's root as the archive flight is, turns them all:
// a frame redraws the root and the dock, while the sidebar's cached drawing
// is used again. A row that isn't drawn again (the sidebar left as it was)
// keeps its place; one that's gone with its sidebar (the panel closed or
// another tab in front) is dropped once it hasn't been seen for `GONE`, and
// to see them the sidebar is asked to draw again every `CONFIRM`.
use crate::style::{self, px};
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TURN_SECONDS: f32 = 0.9;
const FRAME: Duration = Duration::from_millis(20);
/// How often the view an arc is in is asked to draw again, to say its arcs
/// are still there.
const CONFIRM: Duration = Duration::from_secs(1);
/// An arc not drawn by its view for this long has gone with it.
const GONE: Duration = Duration::from_millis(2500);

#[derive(IntoElement)]
pub(crate) struct TurningArc {
    side: f32,
    color: Hsla,
    layered: bool,
}

impl TurningArc {
    /// `side` in points, before the zoom.
    pub(crate) fn new(side: f32, color: Hsla) -> Self {
        Self { side, color, layered: false }
    }

    /// Turned by the window's layer rather than by drawing its view again.
    /// Only for an arc in the window's own content: one in something drawn
    /// over it (a card, a menu) would be turned under that.
    pub(crate) fn layered(mut self) -> Self {
        self.layered = true;
        self
    }
}

impl RenderOnce for TurningArc {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (color, layered) = (self.color, self.layered);
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let id = window.window_handle().window_id();
                if layered && LAYERED.with(|l| l.borrow().contains(&id)) {
                    place(id, window.current_view(), bounds, window.content_mask(), color);
                } else {
                    // A window with no layer (a panel in one of its own)
                    // turns its arcs itself, redrawing their view.
                    paint_arc(bounds, color, window);
                    next_frame(window, cx);
                }
            },
        )
        .flex_none()
        .size(px(self.side))
    }
}

fn paint_arc(bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    static START: OnceLock<Instant> = OnceLock::new();
    let turned = (START.get_or_init(Instant::now).elapsed().as_secs_f32() / TURN_SECONDS).fract();
    // The ring as Swift's: 1.6 wide, 3 in from the sides, zoomed like an
    // icon's strokes.
    let width = style::zoomed(1.6);
    let side = f32::from(bounds.size.width).min(f32::from(bounds.size.height));
    let radius = ((side - style::zoomed(3.)) / 2.).max(1.);
    let center = bounds.center();
    let at = |angle: f32| point(center.x + gpui_kit::px(radius * angle.cos()), center.y + gpui_kit::px(radius * angle.sin()));
    let start = turned * std::f32::consts::TAU;
    let end = start + 0.75 * std::f32::consts::TAU;
    let mut path = PathBuilder::stroke(gpui_kit::px(width));
    path.move_to(at(start));
    // Clockwise on screen (y runs down), the long way round.
    path.arc_to(point(gpui_kit::px(radius), gpui_kit::px(radius)), gpui_kit::px(0.), true, true, at(end));
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
    // Round ends: lyon's caps aren't GPUI's to set, so a dot at each.
    for angle in [start, end] {
        let c = at(angle);
        let dot = Bounds { origin: point(c.x - gpui_kit::px(width / 2.), c.y - gpui_kit::px(width / 2.)), size: size(gpui_kit::px(width), gpui_kit::px(width)) };
        window.paint_quad(fill(dot, color).corner_radii(gpui_kit::px(width / 2.)));
    }
}

thread_local! {
    /// The views with a frame on its way for their arcs.
    static TICKING: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// The windows a layer draws the arcs of.
    static LAYERED: RefCell<HashSet<WindowId>> = RefCell::new(HashSet::new());
    /// The arcs each window's layer draws.
    static PLACED: RefCell<HashMap<WindowId, Vec<Placed>>> = RefCell::new(HashMap::new());
    /// Counts the frames layers draw, so a view's first arc in a frame can
    /// clear the ones it placed before.
    static FRAMES: Cell<u64> = const { Cell::new(0) };
    /// The frame each view last placed arcs in.
    static PLACED_IN: RefCell<HashMap<EntityId, u64>> = RefCell::new(HashMap::new());
    /// When each view was last asked to draw again to confirm its arcs.
    static ASKED: RefCell<HashMap<EntityId, Instant>> = RefCell::new(HashMap::new());
}

/// An arc as its view last drew it: where, clipped to what, in what colour.
#[derive(Clone)]
struct Placed {
    view: EntityId,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    color: Hsla,
    seen: Instant,
}

/// Notes an arc for the window's layer to draw. A view drawing again places
/// all its arcs again, so its first in a frame clears the ones from before.
fn place(window: WindowId, view: EntityId, bounds: Bounds<Pixels>, mask: ContentMask<Pixels>, color: Hsla) {
    let frame = FRAMES.get();
    let fresh = PLACED_IN.with(|p| p.borrow_mut().insert(view, frame) != Some(frame));
    PLACED.with(|p| {
        let mut placed = p.borrow_mut();
        let arcs = placed.entry(window).or_default();
        if fresh {
            arcs.retain(|a| a.view != view);
        }
        arcs.push(Placed { view, bounds, mask, color, seen: Instant::now() });
    });
}

/// The window's arcs, turned: last over the window's own content, under
/// anything deferred (menus, tooltips, the archive flight). Put it after the
/// dock in the window's root view, which it has draw again for each turn.
pub(crate) fn layer() -> impl IntoElement {
    canvas(
        // Before anything's painted this frame, so the arcs placed while the
        // sidebar paints are this frame's.
        |_, window, _| {
            LAYERED.with(|l| l.borrow_mut().insert(window.window_handle().window_id()));
            FRAMES.set(FRAMES.get() + 1);
        },
        |_, _, window, cx| paint_layer(window, cx),
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

fn paint_layer(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let now = Instant::now();
    let arcs = PLACED.with(|p| {
        let mut placed = p.borrow_mut();
        let arcs = placed.entry(id).or_default();
        arcs.retain(|a| now.duration_since(a.seen) < GONE);
        arcs.clone()
    });
    if arcs.is_empty() {
        return;
    }
    for arc in &arcs {
        window.with_content_mask(Some(arc.mask.clone()), |window| paint_arc(arc.bounds, arc.color, window));
    }
    // A view whose arcs haven't been drawn for a while draws again, so
    // they're placed afresh, or dropped if it's not on screen any more.
    let stale: HashSet<EntityId> = arcs.iter().filter(|a| now.duration_since(a.seen) >= CONFIRM).map(|a| a.view).collect();
    for view in stale {
        let due = ASKED.with(|a| {
            let mut asked = a.borrow_mut();
            let due = asked.get(&view).is_none_or(|at| now.duration_since(*at) >= CONFIRM);
            if due {
                asked.insert(view, now);
            }
            due
        });
        if due {
            cx.notify(view);
        }
    }
    // The next turn: the root view, which this is drawn in, draws again.
    next_frame(window, cx);
}

/// The view this arc is in draws again in a moment, once for all its arcs.
fn next_frame(window: &mut Window, cx: &mut App) {
    let view = window.current_view();
    if !TICKING.with(|t| t.borrow_mut().insert(view)) {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(FRAME).await;
        TICKING.with(|t| t.borrow_mut().remove(&view));
        let _ = cx.update(|cx| cx.notify(view));
    })
    .detach();
}
