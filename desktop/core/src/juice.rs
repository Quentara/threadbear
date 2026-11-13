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
    fn the_lift_starts_late_and_lands() {
        let (fade, below) = send_lift(Duration::from_millis(30), false);
        assert_eq!((fade, below), (0., SEND_LIFT_RISE));
        let (fade, below) = send_lift(SEND_LIFT_DELAY + SEND_LIFT, false);
        assert_eq!((fade, below), (1., 0.));
        assert!(!send_lifting(SEND_LIFT_DELAY + SEND_LIFT, false));
        assert_eq!(send_lift(Duration::from_millis(50), true), (0.5, 0.));
    }
}
