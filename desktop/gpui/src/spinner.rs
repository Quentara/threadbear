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
// Even at that rate a turn redrew the whole sidebar, building every row: a
// third of a core while any thread worked (Mike, 13 Nov: "everything does
// seem to slow down a bit once the system loads up"). So the sidebar's arcs
// (`layered`) aren't drawn where they sit. Each notes its place, clip, colour
// and opacity as its row is drawn, and the window's `ArcLayer`, a small view
// of its own drawn last at the window's root as the archive flight is, turns
// them all. A turn asks only that view to draw again: the root draws with it,
// and the sidebar's cached drawing is used as it was. (Asking the app's own
// view would have the sidebar, which watches it, draw again every turn too.)
//
// A sidebar drawing again forgets its arcs and places the ones it still has.
// One that hasn't drawn for `CONFIRM` is asked to, which also shows whether
// it's still on screen: if it doesn't draw within a couple of frames (its
// panel closed, another tab in front) its arcs go. The same check runs when
// the dock's layout changes.
use crate::style::{self, px};
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TURN_SECONDS: f32 = 0.9;
const FRAME: Duration = Duration::from_millis(20);
/// How often a view with arcs is asked to draw again, to show it's still on
/// screen and its arcs are where it last put them.
const CONFIRM: Duration = Duration::from_millis(500);
/// How much of the sidebar list's foot fades into the background
/// (row_motion.rs's FOOT_FADE): an arc in it fades as its row does.
const FOOT_FADE: f32 = 34.;
/// A window whose layer hasn't drawn for this long has closed: its arcs go.
const WINDOW_GONE: Duration = Duration::from_secs(10);

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

    /// Turned by the window's layer rather than by drawing its view again:
    /// for a row in the sidebar's list, whose foot fades. Not for an arc in
    /// something drawn over the window's content (a card, a menu), which the
    /// layer would turn under it, and only in a view that calls
    /// `view_drawing` as it draws.
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
                // Layered only where its view has said it's drawing this frame
                // (`view_drawing`), so it clears them next time: not in the
                // dock's drag ghost, which draws the sidebar under the dock's
                // name and turns its arcs itself as it's drawn every frame.
                if layered && LAYERED.with(|l| l.borrow().contains_key(&id)) && drawing_now(window.current_view()) {
                    // `opacity` scales the alpha: the row's own opacity, once.
                    let color = color.opacity(window.element_opacity());
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
    /// The windows with a layer, and when it last drew.
    static LAYERED: RefCell<HashMap<WindowId, Instant>> = RefCell::new(HashMap::new());
    /// The arcs each window's layer draws.
    static PLACED: RefCell<HashMap<WindowId, Vec<Placed>>> = RefCell::new(HashMap::new());
    /// Counts the frames layers draw: one ends as its layer's paint does.
    static FRAMES: Cell<u64> = const { Cell::new(0) };
    /// The frame each view with layered arcs last drew in, and when.
    static DRAWN: RefCell<HashMap<EntityId, (u64, Instant)>> = RefCell::new(HashMap::new());
    /// Views asked to draw again: the frame they were asked in, and when.
    static ASKED: RefCell<HashMap<EntityId, (u64, Instant)>> = RefCell::new(HashMap::new());
    /// Every view with arcs is to be asked at the next turn (the dock changed).
    static ASK_ALL: Cell<bool> = const { Cell::new(false) };
}

/// An arc as its view last drew it: where, clipped to what, in what colour
/// (its row's opacity in it).
#[derive(Clone, Debug, PartialEq)]
struct Placed {
    view: EntityId,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    color: Hsla,
}

/// A view with layered arcs is drawing: it places the ones it still has as it
/// paints, so the ones from before go. Call it from the view's render.
pub(crate) fn view_drawing(window: &Window) {
    let view = window.current_view();
    DRAWN.with(|d| d.borrow_mut().insert(view, (FRAMES.get(), Instant::now())));
    PLACED.with(|p| {
        if let Some(arcs) = p.borrow_mut().get_mut(&window.window_handle().window_id()) {
            arcs.retain(|a| a.view != view);
        }
    });
}

/// Whether `view` has said it's drawing in this frame.
fn drawing_now(view: EntityId) -> bool {
    DRAWN.with(|d| d.borrow().get(&view).is_some_and(|(frame, _)| *frame == FRAMES.get()))
}

fn place(window: WindowId, view: EntityId, bounds: Bounds<Pixels>, mask: ContentMask<Pixels>, color: Hsla) {
    PLACED.with(|p| p.borrow_mut().entry(window).or_default().push(Placed { view, bounds, mask, color }));
}

/// The window's arcs, turned: drawn last over the window's own content, and
/// under anything deferred (menus, tooltips, the archive flight). One per
/// window, after the dock in its root view.
pub(crate) struct ArcLayer {
    /// The dock's layout when last drawn, to notice it changing.
    dock: String,
}

impl ArcLayer {
    pub(crate) fn new() -> Self {
        Self { dock: String::new() }
    }

    /// The dock's layout as `summary` says it: a change (a panel closed, a
    /// tab brought in front) has the sidebar show whether it's still there.
    pub(crate) fn dock_is(&mut self, summary: String) {
        if summary != self.dock {
            self.dock = summary;
            ASK_ALL.set(true);
        }
    }
}

impl Render for ArcLayer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        canvas(
            |_, window, _| {
                LAYERED.with(|l| l.borrow_mut().insert(window.window_handle().window_id(), Instant::now()));
            },
            |_, _, window, cx| paint_layer(window, cx),
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

/// What a layer does at the end of a frame, worked out apart from drawing so
/// it can be tested: which views' arcs go (asked to draw and didn't), and
/// which to ask now.
#[derive(Debug, Default, PartialEq)]
struct Turn {
    gone: Vec<EntityId>,
    ask: Vec<EntityId>,
}

/// `frame` is the one ending; `views` those with arcs in this window.
///
/// GPUI only redraws a view it was asked to in a window whose last finished
/// frame had it, so a view is asked only once a frame has passed since it
/// drew: one that has just come back (its tab brought in front) would
/// otherwise miss the ask and lose the arcs it had only just placed.
fn turn(frame: u64, now: Instant, views: &HashSet<EntityId>, drawn: &HashMap<EntityId, (u64, Instant)>, asked: &HashMap<EntityId, (u64, Instant)>, ask_all: bool) -> Turn {
    let mut out = Turn::default();
    for view in views {
        let Some(&(drew, drew_at)) = drawn.get(view) else {
            // Placed without drawing: can't be (place needs it). Let it go.
            out.gone.push(*view);
            continue;
        };
        match asked.get(view) {
            // Asked, and not drawn in the two frames since: not on screen.
            Some((at, _)) if drew <= *at && frame >= at + 2 => out.gone.push(*view),
            // Asked a moment ago and not answered yet: wait.
            Some((at, _)) if drew <= *at => {}
            _ => {
                // Drew this very frame: on screen, and not yet one the window
                // would pass an ask on to.
                if drew >= frame {
                    continue;
                }
                let last = asked.get(view).map_or(drew_at, |(_, when)| (*when).max(drew_at));
                if ask_all || now.duration_since(last) >= CONFIRM {
                    out.ask.push(*view);
                }
            }
        }
    }
    out.gone.sort();
    out.ask.sort();
    out
}

fn paint_layer(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let now = Instant::now();
    let frame = FRAMES.get();
    // Closed windows' arcs go.
    let closed: Vec<WindowId> = LAYERED.with(|l| {
        let mut l = l.borrow_mut();
        let closed = l.iter().filter(|(_, at)| now.duration_since(**at) >= WINDOW_GONE).map(|(w, _)| *w).collect::<Vec<_>>();
        l.retain(|_, at| now.duration_since(*at) < WINDOW_GONE);
        closed
    });
    let views: HashSet<EntityId> = PLACED.with(|p| {
        let mut p = p.borrow_mut();
        for w in &closed {
            p.remove(w);
        }
        p.get(&id).map(|arcs| arcs.iter().map(|a| a.view).collect()).unwrap_or_default()
    });
    let plan = DRAWN.with(|d| ASKED.with(|a| turn(frame, now, &views, &d.borrow(), &a.borrow(), ASK_ALL.get())));
    ASK_ALL.set(false);
    if !plan.gone.is_empty() {
        PLACED.with(|p| {
            if let Some(arcs) = p.borrow_mut().get_mut(&id) {
                arcs.retain(|a| !plan.gone.contains(&a.view));
            }
        });
        ASKED.with(|a| plan.gone.iter().for_each(|v| _ = a.borrow_mut().remove(v)));
        DRAWN.with(|d| plan.gone.iter().for_each(|v| _ = d.borrow_mut().remove(v)));
    }
    for view in &plan.ask {
        ASKED.with(|a| a.borrow_mut().insert(*view, (frame, now)));
        cx.notify(*view);
    }
    // What's kept of views that have no arcs anywhere now (their last
    // working thread done, their window closed) goes.
    let placed: HashSet<EntityId> = PLACED.with(|p| p.borrow().values().flatten().map(|a| a.view).collect());
    DRAWN.with(|d| d.borrow_mut().retain(|v, _| placed.contains(v)));
    ASKED.with(|a| a.borrow_mut().retain(|v, _| placed.contains(v)));
    let arcs = PLACED.with(|p| p.borrow().get(&id).cloned().unwrap_or_default());
    for arc in &arcs {
        // At the sidebar list's foot its rows fade into the background.
        let bottom = f32::from(arc.mask.bounds.bottom());
        let shown = ((bottom - f32::from(arc.bounds.center().y)) / FOOT_FADE).clamp(0., 1.);
        let color = arc.color.opacity(shown);
        window.with_content_mask(Some(arc.mask.clone()), |window| paint_arc(arc.bounds, color, window));
    }
    FRAMES.set(frame + 1);
    // The next turn: this layer's view draws again (and the root with it),
    // while there are arcs or a view still to answer.
    if !arcs.is_empty() || !plan.ask.is_empty() {
        next_frame(window, cx);
    }
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

#[cfg(test)]
mod tests {
    // By name: a glob would bring in GPUI's own `test` attribute in place of Rust's.
    use super::{turn, Turn, CONFIRM};
    use gpui_kit::EntityId;
    use std::collections::{HashMap, HashSet};
    use std::time::{Duration, Instant};

    fn view(n: u64) -> EntityId {
        EntityId::from(n)
    }

    #[test]
    fn a_view_drawn_lately_is_left_alone() {
        let now = Instant::now();
        let views = HashSet::from([view(1)]);
        let drawn = HashMap::from([(view(1), (10, now - Duration::from_millis(100)))]);
        assert_eq!(turn(11, now, &views, &drawn, &HashMap::new(), false), Turn::default());
    }

    #[test]
    fn a_view_not_drawn_for_a_while_is_asked() {
        let now = Instant::now();
        let views = HashSet::from([view(1)]);
        let drawn = HashMap::from([(view(1), (10, now - CONFIRM))]);
        assert_eq!(turn(20, now, &views, &drawn, &HashMap::new(), false).ask, vec![view(1)]);
        // Answered an ask lately: not asked again yet.
        let asked = HashMap::from([(view(1), (9, now - Duration::from_millis(100)))]);
        assert_eq!(turn(20, now, &views, &drawn, &asked, false), Turn::default());
    }

    #[test]
    fn a_view_isnt_asked_in_the_frame_it_drew() {
        // Just back on screen: the window wouldn't pass the ask on yet.
        let now = Instant::now();
        let views = HashSet::from([view(1)]);
        let drawn = HashMap::from([(view(1), (20, now - CONFIRM))]);
        assert_eq!(turn(20, now, &views, &drawn, &HashMap::new(), true), Turn::default());
        assert_eq!(turn(21, now, &views, &drawn, &HashMap::new(), true).ask, vec![view(1)]);
    }

    #[test]
    fn a_view_that_doesnt_answer_within_two_frames_loses_its_arcs() {
        let now = Instant::now();
        let views = HashSet::from([view(1)]);
        let drawn = HashMap::from([(view(1), (10, now))]);
        let asked = HashMap::from([(view(1), (12, now))]);
        // Frame 13: give it a frame to draw.
        assert_eq!(turn(13, now, &views, &drawn, &asked, false), Turn::default());
        // Frame 14 and still not drawn: its panel's gone.
        assert_eq!(turn(14, now, &views, &drawn, &asked, false).gone, vec![view(1)]);
        // Drawn in answer: kept.
        let answered = HashMap::from([(view(1), (13, now))]);
        assert_eq!(turn(14, now, &views, &answered, &asked, false), Turn::default());
    }

    #[test]
    fn a_dock_change_asks_every_view_at_once() {
        let now = Instant::now();
        let views = HashSet::from([view(1), view(2)]);
        let drawn = HashMap::from([(view(1), (10, now)), (view(2), (10, now))]);
        let asked = HashMap::from([(view(1), (9, now)), (view(2), (9, now))]);
        assert_eq!(turn(11, now, &views, &drawn, &asked, true).ask, vec![view(1), view(2)]);
    }
}
