// The line bear, in parts that move on their own: the Swift app's
// `BearGeometry` (app/Sources/Threadbear/BearGeometry.swift) drawn out as
// SVGs by desktop/scripts/bear-svgs.swift, and the bears that move them
// (juice.rs; the timings are threadbear_core::juice's).
//
// GPUI turns and scales an SVG about the middle of its box, so each part's
// picture is a 48-unit square centred on the point it turns about (the feet,
// the shoulder): drawn for a bear `side` across, its box is `2 * side`
// square with that point where it falls on the bear's own 24-unit grid. A
// part's turn, squash and lift then happen about the right point with no
// more arithmetic. Parts that should hide what's behind them (the head over
// the waving arm) are filled in the background's colour, as an SVG here is
// one colour.
use crate::style::px;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// The waving pose, whole, turning about the bottom of its square.
    Wave,
    WaveSmall,
    /// Sat, for background work; and the cub, for sub-agents.
    Sit,
    Cub,
    /// The working bear: its arm about the shoulder, the head filled to hide
    /// the arm behind it, the face, and the eye that blinks.
    WorkArm,
    WorkHeadFill,
    WorkFace,
    WorkEye,
    /// Asleep: the head, and each Z on its own to drift.
    SleepHead,
    SleepHeadSmall,
    SleepZ1,
    SleepZ2,
    /// Sitting up: eyes shut, then open (all but the eye, and the eye).
    SitShut,
    SitStill,
    SitEye,
    /// The sat bear's outline filled, to hide the stretching arm behind it.
    SitFill,
    StretchArm,
}

impl Part {
    fn svg(self) -> &'static [u8] {
        match self {
            Part::Wave => include_bytes!("../icons/bears/wave.svg"),
            Part::WaveSmall => include_bytes!("../icons/bears/wave-small.svg"),
            Part::Sit => include_bytes!("../icons/bears/sit.svg"),
            Part::Cub => include_bytes!("../icons/bears/cub.svg"),
            Part::WorkArm => include_bytes!("../icons/bears/work-arm.svg"),
            Part::WorkHeadFill => include_bytes!("../icons/bears/work-head-fill.svg"),
            Part::WorkFace => include_bytes!("../icons/bears/work-face.svg"),
            Part::WorkEye => include_bytes!("../icons/bears/work-eye.svg"),
            Part::SleepHead => include_bytes!("../icons/bears/sleep-head.svg"),
            Part::SleepHeadSmall => include_bytes!("../icons/bears/sleep-head-small.svg"),
            Part::SleepZ1 => include_bytes!("../icons/bears/sleep-z1.svg"),
            Part::SleepZ2 => include_bytes!("../icons/bears/sleep-z2.svg"),
            Part::SitShut => include_bytes!("../icons/bears/sit-shut.svg"),
            Part::SitStill => include_bytes!("../icons/bears/sit-still.svg"),
            Part::SitEye => include_bytes!("../icons/bears/sit-eye.svg"),
            Part::SitFill => include_bytes!("../icons/bears/sit-fill.svg"),
            Part::StretchArm => include_bytes!("../icons/bears/stretch-arm.svg"),
        }
    }

    /// The point on the bear's 24-unit grid its picture is centred on, as
    /// bear-svgs.swift draws it.
    fn anchor(self) -> (f32, f32) {
        const BOTTOM: (f32, f32) = (12., 24.);
        const FEET: (f32, f32) = (9., 19.2);
        const SLEEPING_FEET: (f32, f32) = (8.5, 21.8);
        const SIT_FEET: (f32, f32) = (12., 23.5);
        match self {
            Part::Wave | Part::WaveSmall | Part::Sit | Part::Cub => BOTTOM,
            Part::WorkArm => (14.5, 16.),
            Part::WorkHeadFill | Part::WorkFace => FEET,
            // The waving face's left eye: centre (9, 12.76), scale 0.8.
            Part::WorkEye => (9. + (8. - 12.) * 0.8, 12.76 + (11.6 - 13.2) * 0.8),
            Part::SleepHead | Part::SleepHeadSmall | Part::SleepZ1 | Part::SleepZ2 => SLEEPING_FEET,
            Part::SitShut | Part::SitStill | Part::SitEye | Part::SitFill => SIT_FEET,
            Part::StretchArm => (16.6, 14.6),
        }
    }
}

/// How a part is placed this frame: turned (degrees, clockwise), squashed
/// (x, y) and moved (grid units, down is positive) about its anchor, and
/// how much of it shows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pose {
    pub angle: f32,
    pub scale: (f32, f32),
    pub shift: (f32, f32),
    pub opacity: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Pose { angle: 0., scale: (1., 1.), shift: (0., 0.), opacity: 1. }
    }
}

impl Pose {
    pub fn turned(angle: f32) -> Self {
        Pose { angle, ..Pose::default() }
    }
}

/// One part of a bear `side` points across, in `colour`, posed: absolutely
/// placed, for a `bear` box to hold.
pub(crate) fn part(part: Part, side: f32, colour: Hsla, pose: Pose) -> AnyElement {
    let k = side / 24.;
    let (ax, ay) = part.anchor();
    let shift = point(px(pose.shift.0 * k), px(pose.shift.1 * k));
    let transform = Transformation::rotate(radians(pose.angle.to_radians())).with_scaling(size(pose.scale.0, pose.scale.1)).with_translation(shift);
    div()
        .absolute()
        .left(px(ax * k - side))
        .top(px(ay * k - side))
        .child(Icon::default().data(part.svg()).w(px(side * 2.)).h(px(side * 2.)).text_color(colour.opacity(colour.a * pose.opacity.clamp(0., 1.))).transform(transform))
        .into_any_element()
}

/// A box `side` points square for a bear's parts, which may reach past it.
pub(crate) fn bear(side: f32, parts: impl IntoIterator<Item = AnyElement>) -> Div {
    div().relative().flex_none().size(px(side)).children(parts)
}

/// A whole pose that doesn't move.
pub(crate) fn still(pose: Part, side: f32, colour: Hsla) -> Div {
    bear(side, [part(pose, side, colour, Pose::default())])
}
