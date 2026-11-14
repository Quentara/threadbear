// The picture behind the main window (the Swift app's WindowBackdrop and
// WindowBackdropStore): Bierstadt's Sierra Nevada painting, frosted, under
// the activity bar, every panel and the rail, with the panels tinted glass
// over it (BearDock's `PanelFill`).
//
// It's blurred once as it loads, on the background executor, and kept no
// wider than it needs to be once blurred, so drawing it costs no more than
// a plain colour. It's always as tall as the window and centred; a window
// wider than the picture shows it mirrored either side, which meets it
// without a seam.
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::sync::Arc;

/// The painting (Bierstadt, public domain), the same file the Swift app ships.
const PAINTING: &[u8] = include_bytes!("../../../app/Sources/Threadbear/Resources/Art/window-backdrop.jpg");

/// How soft the picture goes, as frosted glass: the Gaussian blur's sigma in
/// pixels of a picture `FROST_WIDTH` wide, as the Swift app's `frostRadius`
/// (10 at 2400). Narrower pictures get proportionally less, never under
/// `MIN_FROST`, so every picture looks as soft once it fills the window.
const FROST: f32 = 10.;
const FROST_WIDTH: f32 = 2400.;
const MIN_FROST: f32 = 1.5;
/// The widest the frosted copy is kept. Once blurred there's no detail a
/// wider one would keep, and stretching it over the window smooths it more.
const DRAWN_WIDTH: u32 = 1200;

/// What's drawn, frosted: the picture and its mirror image, and its shape.
#[derive(Clone)]
pub(crate) struct Frosted {
    pub(crate) picture: Arc<Image>,
    pub(crate) mirrored: Arc<Image>,
    /// Width over height.
    pub(crate) aspect: f32,
}

#[derive(Default)]
pub(crate) struct Backdrop {
    pub(crate) shown: Option<Frosted>,
}

impl Global for Backdrop {}

/// Loads the painting, frosts it off the main thread, and shows it.
pub(crate) fn load(cx: &mut App) {
    cx.set_global(Backdrop::default());
    cx.spawn(async move |cx| {
        let made = cx.background_executor().spawn(async move { frost(PAINTING) }).await;
        let _ = cx.update(|cx| {
            cx.global_mut::<Backdrop>().shown = made;
            cx.refresh_windows();
        });
    })
    .detach();
}

/// The picture behind the window, once it's loaded.
pub(crate) fn shown(cx: &App) -> Option<&Frosted> {
    cx.try_global::<Backdrop>().and_then(|b| b.shown.as_ref())
}

/// What the panels, the activity bar and the rail are filled with over the
/// picture, and the line along the seams between panels (`PanelFill`):
/// dark enough for text in dark mode, a light wash in light mode, as the
/// Swift app's `panelTint` (the darker one, so text stands out over the
/// painting). `None` while there's no picture: the panels keep the
/// window's own colour.
pub(crate) fn glass(cx: &App) -> Option<(Hsla, Hsla)> {
    shown(cx)?;
    Some(glass_colours(cx.theme().is_dark(), cx.theme().foreground))
}

/// The tint and the seam for each look. The seam is the same faint line as
/// under the panels' bars (`Color.primary.opacity(0.07)`).
pub(crate) fn glass_colours(dark: bool, foreground: Hsla) -> (Hsla, Hsla) {
    let tint = if dark { hsla(0., 0., 0.07, 0.78) } else { hsla(0., 0., 0.98, 0.82) };
    (tint, foreground.opacity(0.07))
}

/// What a panel's own view fills itself with: nothing over the picture, so
/// the dock's tint is all there is between it and the painting, else the
/// window's colour as before.
pub(crate) fn ground(cx: &App) -> Hsla {
    if shown(cx).is_some() { transparent_black() } else { cx.theme().background }
}

/// The tint alone, for the activity bar and the rail.
pub(crate) fn tint(cx: &App) -> Option<Hsla> {
    glass(cx).map(|(tint, _)| tint)
}

/// Where each copy of the picture goes across a window `width` wide and
/// `height` tall, as (left edge, width, mirrored): the picture at the
/// window's height in the middle, a mirrored copy either side.
pub(crate) fn tiles(width: f32, height: f32, aspect: f32) -> [(f32, f32, bool); 3] {
    let w = height * aspect;
    let middle = (width - w) / 2.;
    [(middle - w, w, true), (middle, w, false), (middle + w, w, true)]
}

/// The layer under everything in the main window: the window's colour, then
/// the picture. Nothing while there's no picture.
pub(crate) fn layer(window: &Window, cx: &App) -> Option<AnyElement> {
    let frosted = shown(cx)?;
    let size = window.viewport_size();
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let mut layer = div().absolute().top_0().left_0().size_full().overflow_hidden().bg(cx.theme().background);
    for (x, w, mirrored) in tiles(width, height, frosted.aspect) {
        let picture = if mirrored { frosted.mirrored.clone() } else { frosted.picture.clone() };
        layer = layer.child(img(picture).absolute().top_0().left(px(x)).w(px(w)).h(px(height)).object_fit(ObjectFit::Fill));
    }
    Some(layer.into_any_element())
}

/// The picture blurred once, at most `DRAWN_WIDTH` wide, and its mirror
/// image, ready to draw. `None` when it isn't a picture this can read.
pub(crate) fn frost(bytes: &[u8]) -> Option<Frosted> {
    let (picture, mirrored, aspect) = frost_pixels(bytes)?;
    Some(Frosted { picture: png(&picture)?, mirrored: png(&mirrored)?, aspect })
}

fn frost_pixels(bytes: &[u8]) -> Option<(image::RgbaImage, image::RgbaImage, f32)> {
    use image::imageops::{self, FilterType};
    let picture = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = picture.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let aspect = w as f32 / h as f32;
    let picture = if w > DRAWN_WIDTH { imageops::resize(&picture, DRAWN_WIDTH, ((DRAWN_WIDTH as f32 / aspect).round() as u32).max(1), FilterType::Triangle) } else { picture };
    let frosted = imageops::fast_blur(&picture, frost_sigma(w, picture.width()));
    let mirrored = imageops::flip_horizontal(&frosted);
    Some((frosted, mirrored, aspect))
}

/// The blur for a picture `original` pixels wide, kept `kept` wide: as soft
/// as the Swift app makes it at the picture's own size, scaled to the copy.
pub(crate) fn frost_sigma(original: u32, kept: u32) -> f32 {
    let at_own_size = (FROST * original.min(FROST_WIDTH as u32) as f32 / FROST_WIDTH).max(MIN_FROST);
    at_own_size * kept as f32 / original.min(FROST_WIDTH as u32).max(1) as f32
}

fn png(picture: &image::RgbaImage) -> Option<Arc<Image>> {
    let mut bytes = Vec::new();
    picture.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).ok()?;
    Some(Arc::new(Image::from_bytes(gpui_kit::ImageFormat::Png, bytes)))
}

#[cfg(test)]
mod tests {
    // By name: a glob would bring in GPUI's own `test` attribute in place of Rust's.
    use super::{frost, frost_pixels, frost_sigma, tiles, DRAWN_WIDTH, PAINTING};

    #[test]
    fn the_picture_is_as_tall_as_the_window_with_a_mirror_either_side() {
        // A 2:1 picture in a 1000 by 300 window: 600 wide, centred.
        let [left, middle, right] = tiles(1000., 300., 2.);
        assert_eq!(middle, (200., 600., false));
        assert_eq!(left, (-400., 600., true));
        assert_eq!(right, (800., 600., true));
    }

    #[test]
    fn a_narrow_window_cuts_the_sides_off() {
        let [_, middle, _] = tiles(400., 300., 2.);
        assert_eq!(middle, (-100., 600., false));
    }

    #[test]
    fn the_painting_loads_frosted_and_mirrored() {
        let (frosted, mirrored, aspect) = frost_pixels(PAINTING).expect("the painting decodes");
        assert_eq!(frosted.width(), DRAWN_WIDTH);
        assert!((aspect - 2400. / 1543.).abs() < 0.001);
        assert_eq!(frosted.get_pixel(0, 10), mirrored.get_pixel(DRAWN_WIDTH - 1, 10));
    }

    #[test]
    fn every_picture_is_as_soft_once_it_fills_the_window() {
        // The painting: 10 at 2400, so 5 on the 1200 copy.
        assert_eq!(frost_sigma(2400, 1200), 5.);
        // A camera's 6000-pixel photo is as soft as the painting.
        assert_eq!(frost_sigma(6000, 1200), 5.);
        // A small picture kept as it is gets less, but never under 1.5.
        assert!((frost_sigma(800, 800) - 10. / 3.).abs() < 0.001);
        assert_eq!(frost_sigma(200, 200), 1.5);
    }

    #[test]
    fn something_that_isnt_a_picture_shows_nothing() {
        assert!(frost(b"not a picture").is_none());
    }
}
