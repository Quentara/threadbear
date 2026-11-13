// The small motions' timings and curves: the Swift app's `Juice` and the
// keyframes its views animate with (app/Sources/Threadbear/Views/Juice.swift,
// SidebarView, ComposerControls, WavingBearView, InkLayers), as plain numbers
// the GPUI app draws from (desktop/gpui/src/juice.rs, bears.rs). Everything
// here is a function of the time since the motion started, so a frame the
// main thread misses is skipped rather than slowing the motion down.
use std::time::Duration;

/// A CSS-style cubic Bézier timing curve through (0,0), (x1,y1), (x2,y2),
/// (1,1): how far along at `t` of the time, both 0 to 1.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t == 0. || t == 1. {
        return t;
    }
    let curve = |a: f32, b: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * s * a + 3. * u * s * s * b + s * s * s
    };
    // Finds the curve's own parameter whose x is `t`: Newton's steps, then
    // halving if they wander off.
    let mut s = t;
    for _ in 0..8 {
        let x = curve(x1, x2, s) - t;
        if x.abs() < 1e-5 {
            return curve(y1, y2, s);
        }
        let u = 1. - s;
        let dx = 3. * u * u * x1 + 6. * u * s * (x2 - x1) + 3. * s * s * (1. - x2);
        if dx.abs() < 1e-6 {
            break;
        }
        s = (s - x / dx).clamp(0., 1.);
    }
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..30 {
        s = (lo + hi) / 2.;
        if curve(x1, x2, s) < t {
            lo = s;
        } else {
            hi = s;
        }
    }
    curve(y1, y2, s)
}

/// `CAMediaTimingFunction(controlPoints: 0, 0, 0.2, 1)`: quick out of the
/// blocks, settling gently. The send lift and the card settling in use it.
pub fn ease_out_settle(t: f32) -> f32 {
    cubic_bezier(0., 0., 0.2, 1., t)
}

/// A spring from 0 to 1 after `t` seconds, as SwiftUI's
/// `.spring(response:dampingFraction:)`: under 1 it overshoots and settles.
pub fn spring(t: f32, response: f32, damping: f32) -> f32 {
    if t <= 0. {
        return 0.;
    }
    let omega = std::f32::consts::TAU / response;
    if damping >= 1. {
        return 1. - (1. + omega * t) * (-omega * t).exp();
    }
    let root = (1. - damping * damping).sqrt();
    let damped = omega * root;
    1. - (-damping * omega * t).exp() * ((damped * t).cos() + damping / root * (damped * t).sin())
}

/// SwiftUI's `.snappy(duration:)`: a spring with a little bounce whose
/// response is the duration.
pub fn snappy(t: f32, duration: f32) -> f32 {
    spring(t, duration, 0.85)
}

/// A run of keyframes, each `(seconds to get there, value)`, from `start`:
/// eased in and out between each, as SwiftUI's `CubicKeyframe` near enough
/// for motions this small. Past the end it holds the last value.
#[derive(Clone, Copy, Debug)]
pub struct Track<'a> {
    pub start: f32,
    pub keys: &'a [(f32, f32)],
}

impl Track<'_> {
    pub fn duration(&self) -> f32 {
        self.keys.iter().map(|k| k.0).sum()
    }

    pub fn at(&self, t: f32) -> f32 {
        let (mut from, mut elapsed) = (self.start, 0.);
        for &(length, to) in self.keys {
            if t < elapsed + length {
                let u = ((t - elapsed) / length).clamp(0., 1.);
                return from + (to - from) * u * u * (3. - 2. * u);
            }
            elapsed += length;
            from = to;
        }
        from
    }
}

/// Keyframes at set times, each `(seconds from the start, value)`, eased in
/// and out between, as Core Animation's keyframe animations with
/// `keyTimes` (the working bear's tracks). Before the first, its value.
pub fn timed(keys: &[(f32, f32)], t: f32) -> f32 {
    let Some(&(_, first)) = keys.first() else { return 0. };
    let mut before = (0f32, first);
    for &(at, value) in keys {
        if t < at {
            let span = (at - before.0).max(1e-6);
            let u = ((t - before.0) / span).clamp(0., 1.);
            return before.1 + (value - before.1) * u * u * (3. - 2. * u);
        }
        before = (at, value);
    }
    before.1
}

/// How far along a motion of `length` is after `elapsed`, 0 to 1.
pub fn progress(elapsed: Duration, length: Duration) -> f32 {
    (elapsed.as_secs_f32() / length.as_secs_f32().max(1e-6)).clamp(0., 1.)
}

// MARK: The sidebar's selection

/// The selected row's highlight sliding to the next one: 160 ms on
/// `cubic-bezier(.2, .9, .25, 1)`, the curve Mike picked from the juice
/// mockups (13 Nov); none at all under Reduce Motion.
pub const SELECTION_SLIDE: Duration = Duration::from_millis(160);

pub fn selection_slide(t: f32) -> f32 {
    cubic_bezier(0.2, 0.9, 0.25, 1., t)
}

// MARK: The activity bar

/// The lit square sliding between the view buttons (`Juice.selection`:
/// snappy, 0.2 s), and how long it's given to settle.
pub const LIT_RESPONSE: f32 = 0.2;
pub const LIT_SLIDE: Duration = Duration::from_millis(400);

/// A button's icon after a click: pressed in to 0.82 in 0.08 s, then a
/// bouncy spring home (`.bouncy(duration: 0.35, extraBounce: 0.25)`).
pub const RAIL_BOUNCE: Duration = Duration::from_millis(500);

/// Its scale `t` seconds after the click.
pub fn rail_bounce(t: f32) -> f32 {
    const PRESS: f32 = 0.08;
    if t < PRESS {
        return Track { start: 1., keys: &[(PRESS, 0.82)] }.at(t);
    }
    0.82 + 0.18 * spring(t - PRESS, 0.35, 0.45)
}

/// The sidebar's new view settling in as the activity bar switches it: up
/// 6 points as it fades in, snappy over 0.22 s; a 0.12 s fade under Reduce
/// Motion (SidebarView's transition).
pub const VIEW_SETTLE_RISE: f32 = 6.;
pub const VIEW_SETTLE: Duration = Duration::from_millis(400);

/// The new view's (opacity, distance below its place) `t` seconds in.
pub fn view_settle(t: f32, reduce: bool) -> (f32, f32) {
    if reduce {
        return ((t / 0.12).clamp(0., 1.), 0.);
    }
    let p = snappy(t, 0.22);
    (p.clamp(0., 1.), VIEW_SETTLE_RISE * (1. - p))
}

// MARK: The Librarian's picture

/// The picture's wobble when poked: a squash (x by this, y by 2 minus it)
/// and a tilt from its feet, in degrees (HomePage's keyframes).
pub const WOBBLE: Duration = Duration::from_millis(450);

/// Its (squash, sideways sway in points) `t` seconds after the poke, for a
/// picture `height` tall: the sway is how far its middle moves as it tilts.
pub fn wobble(t: f32, height: f32) -> (f32, f32) {
    let squash = Track { start: 1., keys: &[(0.045, 1.02), (0.11, 1.)] }.at(t);
    let angle = Track { start: 0., keys: &[(0.16, -2.), (0.11, 1.5), (0.1, -0.8), (0.08, 0.)] }.at(t);
    (squash, angle.to_radians().sin() * height / 2.)
}

// MARK: Sending

/// A sent message lifting off the composer into the thread (`SendLift`):
/// it starts a moment after Return, so its first frame is seen, and rises
/// this far as it fades in, on `ease_out_settle`. Under Reduce Motion, a
/// 100 ms fade.
pub const SEND_LIFT_RISE: f32 = 14.;
pub const SEND_LIFT_DELAY: Duration = Duration::from_millis(60);
pub const SEND_LIFT: Duration = Duration::from_millis(170);
pub const SEND_LIFT_REDUCED: Duration = Duration::from_millis(100);

/// The lifting message's (opacity, distance below its place) at `elapsed`
/// since Return.
pub fn send_lift(elapsed: Duration, reduce: bool) -> (f32, f32) {
    if reduce {
        return (progress(elapsed, SEND_LIFT_REDUCED), 0.);
    }
    let t = progress(elapsed.saturating_sub(SEND_LIFT_DELAY), SEND_LIFT);
    let eased = ease_out_settle(t);
    (eased, SEND_LIFT_RISE * (1. - eased))
}

/// Whether a lift started at `elapsed` ago is still going.
pub fn send_lifting(elapsed: Duration, reduce: bool) -> bool {
    elapsed < if reduce { SEND_LIFT_REDUCED } else { SEND_LIFT_DELAY + SEND_LIFT }
}

/// The send arrow's hop as Return lands: up 3 points and back.
pub const SEND_HOP: Duration = Duration::from_millis(120);

/// The arrow's offset (negative is up) `t` seconds after Return.
pub fn send_hop(t: f32) -> f32 {
    Track { start: 0., keys: &[(0.05, -3.), (0.07, 0.)] }.at(t)
}

// MARK: The composer's card

/// A card arriving while the thread's on screen comes up from this far below
/// as it fades in, over 0.2 s on `ease_out_settle` (`ComposerCard`'s
/// arriving card); under Reduce Motion, a 0.15 s fade.
pub const CARD_SETTLE_RISE: f32 = 6.;
pub const CARD_SETTLE: Duration = Duration::from_millis(200);
pub const CARD_SETTLE_REDUCED: Duration = Duration::from_millis(150);

/// The arriving card's (opacity, distance below its place).
pub fn card_settle(elapsed: Duration, reduce: bool) -> (f32, f32) {
    if reduce {
        return (progress(elapsed, CARD_SETTLE_REDUCED), 0.);
    }
    let eased = ease_out_settle(progress(elapsed, CARD_SETTLE));
    (eased, CARD_SETTLE_RISE * (1. - eased))
}

pub fn card_settling(elapsed: Duration, reduce: bool) -> bool {
    elapsed < if reduce { CARD_SETTLE_REDUCED } else { CARD_SETTLE }
}

/// The done card's tick drawing itself on as the card arrives: a beat in,
/// then along its strokes on `cubic-bezier(.3, 0, .2, 1)`; the tick as ever
/// from `TICK_DRAW_END`. Not under Reduce Motion.
pub const TICK_DRAW_DELAY: Duration = Duration::from_millis(40);
pub const TICK_DRAW: Duration = Duration::from_millis(260);
pub const TICK_DRAW_END: Duration = Duration::from_millis(320);

/// How much of the tick's drawn, 0 to 1.
pub fn tick_drawn(elapsed: Duration) -> f32 {
    cubic_bezier(0.3, 0., 0.2, 1., progress(elapsed.saturating_sub(TICK_DRAW_DELAY), TICK_DRAW))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_ends_and_shape() {
        assert_eq!(cubic_bezier(0.2, 0.9, 0.25, 1., 0.), 0.);
        assert_eq!(cubic_bezier(0.2, 0.9, 0.25, 1., 1.), 1.);
        // That curve is most of the way there by a third of the time.
        assert!(selection_slide(0.33) > 0.8, "{}", selection_slide(0.33));
        // A straight line stays one.
        for i in 0..=10 {
            let t = i as f32 / 10.;
            assert!((cubic_bezier(1. / 3., 1. / 3., 2. / 3., 2. / 3., t) - t).abs() < 1e-3);
        }
        // Never runs backwards.
        let mut last = 0.;
        for i in 0..=100 {
            let v = ease_out_settle(i as f32 / 100.);
            assert!(v + 1e-4 >= last);
            last = v;
        }
    }

    #[test]
    fn springs_settle() {
        assert_eq!(spring(0., 0.3, 0.55), 0.);
        assert!((spring(2., 0.3, 0.55) - 1.).abs() < 1e-3);
        // A bouncy one goes past the end on its way.
        assert!((0..100).any(|i| spring(i as f32 / 100., 0.3, 0.55) > 1.05));
        assert!((snappy(1., 0.2) - 1.).abs() < 1e-3);
        assert!((spring(1., 0.3, 1.) - 1.).abs() < 1e-3);
    }

    #[test]
    fn tracks_hit_their_keys() {
        let wave = Track { start: 0., keys: &[(0.25, 0.), (0.14, -9.), (0.16, 7.), (0.14, -5.), (0.2, 0.)] };
        assert!((wave.duration() - 0.89).abs() < 1e-5);
        assert_eq!(wave.at(0.1), 0.);
        assert!((wave.at(0.39) - -9.).abs() < 1e-3);
        assert!((wave.at(0.55) - 7.).abs() < 1e-3);
        assert_eq!(wave.at(5.), 0.);
        assert!((timed(&[(0., 0.), (0.16, 14.), (0.38, 3.)], 0.16) - 14.).abs() < 1e-3);
        assert_eq!(timed(&[(0., 1.), (1., 2.)], 9.), 2.);
    }

    #[test]
    fn the_rail_bounces_home() {
        assert_eq!(rail_bounce(0.), 1.);
        assert!((rail_bounce(0.08) - 0.82).abs() < 1e-3);
        assert!((0..50).any(|i| rail_bounce(0.08 + i as f32 / 100.) > 1.01));
        assert!((rail_bounce(RAIL_BOUNCE.as_secs_f32()) - 1.).abs() < 0.03);
        assert_eq!(view_settle(0., false), (0., VIEW_SETTLE_RISE));
        assert!(view_settle(VIEW_SETTLE.as_secs_f32(), false).1.abs() < 0.1);
        assert_eq!(view_settle(1., true), (1., 0.));
    }

    #[test]
    fn the_card_settles_and_its_tick_draws() {
        assert_eq!(card_settle(Duration::ZERO, false), (0., CARD_SETTLE_RISE));
        assert_eq!(card_settle(CARD_SETTLE, false), (1., 0.));
        assert_eq!(card_settle(Duration::from_millis(75), true).1, 0.);
        assert_eq!(tick_drawn(Duration::from_millis(20)), 0.);
        assert_eq!(tick_drawn(TICK_DRAW_DELAY + TICK_DRAW), 1.);
        assert!(TICK_DRAW_END >= TICK_DRAW_DELAY + TICK_DRAW);
    }

    #[test]
    fn the_lift_starts_late_and_lands() {
        let (fade, below) = send_lift(Duration::from_millis(30), false);
        assert_eq!((fade, below), (0., SEND_LIFT_RISE));
        let (fade, below) = send_lift(SEND_LIFT_DELAY + SEND_LIFT, false);
        assert_eq!((fade, below), (1., 0.));
        assert!(!send_lifting(SEND_LIFT_DELAY + SEND_LIFT, false));
        assert_eq!(send_lift(Duration::from_millis(50), true), (0.5, 0.));
    }
}
