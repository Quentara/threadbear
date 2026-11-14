// The picture behind the main window (the Swift app's WindowBackdrop and
// WindowBackdropStore): Bierstadt's Sierra Nevada painting, or a picture
// chosen in Settings > General > Appearance, frosted, under the activity
// bar, every panel and the rail, with the panels tinted glass over it
// (BearDock's `PanelFill`).
//
// A chosen picture is copied into the runner's data folder as
// `window-backdrop.<ext>`, the file the Swift app keeps there, so one chosen
// in either app shows in both and stays when the original moves. Going
// back to the painting deletes it.
//
// It's blurred once as it loads, on the background executor, and kept no
// wider than it needs to be once blurred, so drawing it costs no more than
// a plain colour. It's always as tall as the window and centred; a window
// wider than the picture shows it mirrored either side, which meets it
// without a seam.
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use threadbear_client::config::RunnerConfig;

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

/// The name a chosen picture is kept under in the data folder, before its
/// extension (the Swift app's `window-backdrop.<ext>`).
const OWN_NAME: &str = "window-backdrop";
/// The thumbnail in Settings, in pixels (72 by 44 points, at 2x).
const PREVIEW: (u32, u32) = (144, 88);

/// What's drawn, frosted: the picture and its mirror image, and its shape.
#[derive(Clone)]
pub(crate) struct Frosted {
    pub(crate) picture: Arc<Image>,
    pub(crate) mirrored: Arc<Image>,
    /// Width over height.
    pub(crate) aspect: f32,
    /// The picture unblurred and small, for the thumbnail in Settings.
    pub(crate) preview: Arc<Image>,
    /// A picture chosen in Settings rather than the painting.
    pub(crate) own: bool,
}

#[derive(Default)]
pub(crate) struct Backdrop {
    pub(crate) shown: Option<Frosted>,
}

impl Global for Backdrop {}

/// Loads the chosen picture, or the painting, frosts it off the main
/// thread, and shows it. Again after a picture's chosen or let go.
pub(crate) fn load(cx: &mut App) {
    if cx.try_global::<Backdrop>().is_none() {
        cx.set_global(Backdrop::default());
    }
    let home = RunnerConfig::from_env().home;
    cx.spawn(async move |cx| {
        let made = cx.background_executor().spawn(async move { load_from(&home) }).await;
        let _ = cx.update(|cx| {
            cx.global_mut::<Backdrop>().shown = made;
            cx.refresh_windows();
        });
    })
    .detach();
}

/// The picture chosen in `home`, frosted, else the painting. A chosen one
/// that can't be read any more gives way to the painting.
fn load_from(home: &Path) -> Option<Frosted> {
    let own = own_picture(home).and_then(|path| read_picture(&path)).and_then(|picture| frost(&picture, true));
    own.or_else(|| frost(&image::load_from_memory(PAINTING).ok()?, false))
}

/// The picture chosen in Settings, kept in `home`: the newest
/// `window-backdrop.<ext>` there, if any.
pub(crate) fn own_picture(home: &Path) -> Option<PathBuf> {
    own_pictures(home).into_iter().max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn own_pictures(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(home) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.file_stem().is_some_and(|s| s == OWN_NAME) && p.extension().is_some())
        .collect()
}

/// A picture file, decoded: as it is, or on a Mac through AppKit (NSImage,
/// as the Swift app reads it) for what the image crate can't read (HEIC,
/// TIFF), handed over as a PNG.
fn read_picture(path: &Path) -> Option<image::DynamicImage> {
    if let Some(picture) = std::fs::read(path).ok().and_then(|bytes| image::load_from_memory(&bytes).ok()) {
        return Some(picture);
    }
    #[cfg(target_os = "macos")]
    return mac_png(path).and_then(|png| image::load_from_memory(&png).ok());
    #[cfg(not(target_os = "macos"))]
    None
}

/// The picture at `path` as PNG bytes, decoded by AppKit, which reads every
/// format macOS does.
#[cfg(target_os = "macos")]
fn mac_png(path: &Path) -> Option<Vec<u8>> {
    use objc2::AllocAnyThread as _;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSString};
    let picture = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path.to_str()?))?;
    let tiff = picture.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    // SAFETY: no properties, so none of the wrong type.
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(png.to_vec())
}

/// Why a picture couldn't be used, in the alert's words.
pub(crate) const NOT_A_PICTURE: &str = "It isn't a picture Threadbear can read. Try a JPEG, PNG or HEIC.";

/// Copies `source` into `home` as the background and lets go of the one
/// before, whatever its extension. Refuses what isn't a picture, leaving the
/// one before as it was.
pub(crate) fn use_own(source: &Path, home: &Path) -> Result<(), String> {
    read_picture(source).filter(|p| p.width() > 0 && p.height() > 0).ok_or(NOT_A_PICTURE)?;
    let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| !e.is_empty()).unwrap_or_else(|| "png".into());
    std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
    let target = home.join(format!("{OWN_NAME}.{ext}"));
    // Copied to the side first, so a failed copy leaves the one before.
    let partial = home.join(format!(".{OWN_NAME}.partial"));
    std::fs::copy(source, &partial).map_err(|e| e.to_string())?;
    for old in own_pictures(home) {
        let _ = std::fs::remove_file(old);
    }
    std::fs::rename(&partial, &target).map_err(|e| e.to_string())
}

/// Back to the painting: the chosen copy is deleted.
pub(crate) fn use_painting(home: &Path) {
    for old in own_pictures(home) {
        let _ = std::fs::remove_file(old);
    }
}

/// Settings' Choose…: the open panel, then the picture, shown straight
/// away; an alert says why when it can't be used.
pub(crate) fn choose(window: &mut Window, cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Use as background".into()) });
    window
        .spawn(cx, async move |cx| {
            let Ok(Ok(Some(mut paths))) = paths.await else { return };
            let Some(path) = paths.pop() else { return };
            let home = RunnerConfig::from_env().home;
            let used = cx.background_executor().spawn(async move { use_own(&path, &home) }).await;
            let _ = cx.update(|window, cx| match used {
                Ok(()) => load(cx),
                Err(why) => {
                    let _ = window.prompt(PromptLevel::Warning, "Couldn't use that picture", Some(&why), &["OK"], cx);
                }
            });
        })
        .detach();
}

/// Settings' Use the painting.
pub(crate) fn go_back_to_painting(cx: &mut App) {
    use_painting(&RunnerConfig::from_env().home);
    load(cx);
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
/// image, ready to draw, with a small sharp copy for Settings. `None` when
/// it isn't a picture this can read.
pub(crate) fn frost(picture: &image::DynamicImage, own: bool) -> Option<Frosted> {
    let (frosted, mirrored, aspect) = frost_pixels(picture)?;
    // The thumbnail: the picture filling `PREVIEW`, its middle kept.
    let preview = picture.resize_to_fill(PREVIEW.0, PREVIEW.1, image::imageops::FilterType::Triangle).to_rgba8();
    Some(Frosted { picture: png(&frosted)?, mirrored: png(&mirrored)?, aspect, preview: png(&preview)?, own })
}

fn frost_pixels(picture: &image::DynamicImage) -> Option<(image::RgbaImage, image::RgbaImage, f32)> {
    use image::imageops::{self, FilterType};
    let (w, h) = (picture.width(), picture.height());
    if w == 0 || h == 0 {
        return None;
    }
    let aspect = w as f32 / h as f32;
    let picture = if w > DRAWN_WIDTH { imageops::resize(picture, DRAWN_WIDTH, ((DRAWN_WIDTH as f32 / aspect).round() as u32).max(1), FilterType::Triangle) } else { picture.to_rgba8() };
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
    use super::{frost_pixels, frost_sigma, load_from, own_pictures, tiles, use_own, use_painting, DRAWN_WIDTH, NOT_A_PICTURE, PAINTING};
    use std::path::PathBuf;

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
        let (frosted, mirrored, aspect) = frost_pixels(&image::load_from_memory(PAINTING).expect("the painting decodes")).unwrap();
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


    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("tb-backdrop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    #[test]
    fn a_chosen_picture_is_copied_in_and_shown_until_going_back_to_the_painting() {
        let home = temp_home("choose");
        assert!(!load_from(&home).unwrap().own);
        let source = home.join("holiday.JPG");
        std::fs::write(&source, PAINTING).unwrap();
        // One chosen before, as a PNG, goes.
        std::fs::write(home.join("window-backdrop.png"), b"old").unwrap();
        use_own(&source, &home).unwrap();
        assert_eq!(own_pictures(&home), vec![home.join("window-backdrop.jpg")]);
        assert!(load_from(&home).unwrap().own);
        use_painting(&home);
        assert!(own_pictures(&home).is_empty());
        assert!(!load_from(&home).unwrap().own);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn something_that_isnt_a_picture_is_refused_and_the_one_before_stays() {
        let home = temp_home("refuse");
        let source = home.join("notes.png");
        std::fs::write(&source, b"not a picture").unwrap();
        std::fs::write(home.join("window-backdrop.jpg"), PAINTING).unwrap();
        assert_eq!(use_own(&source, &home), Err(NOT_A_PICTURE.to_string()));
        assert_eq!(own_pictures(&home), vec![home.join("window-backdrop.jpg")]);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_chosen_picture_that_cant_be_read_any_more_gives_way_to_the_painting() {
        let home = temp_home("broken");
        std::fs::write(home.join("window-backdrop.png"), b"broken").unwrap();
        assert!(!load_from(&home).unwrap().own);
        let _ = std::fs::remove_dir_all(&home);
    }
}
