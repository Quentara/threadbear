use super::{icon_png, read_icon, Origin};
use std::path::Path;

fn decoded(name: &str) -> image::RgbaImage {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/icons").join(name);
    let checked = read_icon(&path, Origin::ThisMac).unwrap();
    assert!(!checked.is_application());
    let png = icon_png(checked).unwrap();
    image::load_from_memory(&png).unwrap().to_rgba8()
}

#[test]
fn a_small_pdf_icon_fills_the_rail_image_and_uses_the_crop_box() {
    let image = decoded("Small.pdf");
    assert_eq!(image.dimensions(), (128, 128));
    let filled = image.pixels().filter(|p| p[3] > 240 && p[0] > 200).count();
    assert!(filled > 16_000, "The vector icon filled only {filled} pixels");
}

#[test]
fn a_rotated_pdf_icon_keeps_its_shape_and_clockwise_rotation() {
    let image = decoded("Rotated.pdf");
    assert!(image.get_pixel(64, 8)[3] < 10, "The rotated rectangle should have padding above it");
    let left = image.get_pixel(8, 64);
    let right = image.get_pixel(120, 64);
    assert!(left[2] > 200 && left[0] < 80, "The original bottom half should rotate left: {left:?}");
    assert!(right[0] > 200 && right[2] < 80, "The original top half should rotate right: {right:?}");
}

#[test]
fn an_icns_uses_the_large_blue_representation_instead_of_the_small_red_one() {
    let image = decoded("Multi.icns");
    let center = image.get_pixel(64, 64);
    assert!(center[2] > 220 && center[0] < 80, "The large representation is blue: {center:?}");
}

#[test]
fn an_icon_from_another_mac_never_reads_the_same_local_path() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/icons/Small.pdf");
    assert!(read_icon(&path, Origin::OtherMac).is_err());
}

#[test]
fn an_icns_cannot_hide_an_enormous_png_behind_its_representation_type() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/icons/Huge.icns");
    let checked = read_icon(&path, Origin::ThisMac).unwrap();
    assert!(icon_png(checked).is_err());
}
