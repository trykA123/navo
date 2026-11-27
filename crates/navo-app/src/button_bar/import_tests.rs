//! Tests for the button bar icon importer.

use std::fs;
use std::path::{Path, PathBuf};

use navo_icons::raster::RASTER_CAP;
use navo_icons::sanitise::SVG_CAP;
use navo_icons::spec::IconSpec;

use super::import;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
    <rect x="2" y="2" width="20" height="20" fill="#123456"/>
    <text x="4" y="16">Hello</text>
</svg>"##;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("a temporary folder");
    let source = root.path().join("source");
    let icons = root.path().join("buttonbar-icons");
    fs::create_dir_all(&source).expect("a source folder");
    (root, source, icons)
}

fn entries(dir: &Path) -> Vec<String> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .map(|entry| entry.expect("a directory entry"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn hashed(stem: &str, ext: &str, name: &str) -> bool {
    name.starts_with(&format!("{stem}-"))
        && name.ends_with(&format!(".{ext}"))
        && name.len() == stem.len() + 1 + 8 + 1 + ext.len()
        && name[stem.len() + 1..stem.len() + 9]
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

#[test]
fn import_svg_writes_hashed_copy() {
    let (_root, source, icons) = setup();
    let path = source.join("star.svg");
    fs::write(&path, SVG).expect("write svg");

    let got = import(&path, &icons, true).expect("svg import");
    let stored = match &got.spec {
        IconSpec::Svg { file, mono } => {
            assert!(*mono);
            file.clone()
        }
        other => panic!("expected an SVG spec, got {other:?}"),
    };
    assert!(hashed("star", "svg", &stored), "unexpected name {stored}");
    let written = fs::read_to_string(icons.join(&stored)).expect("read stored svg");
    assert!(!written.to_ascii_lowercase().contains("<text"));
    assert_eq!(got.removed, vec!["text".to_owned()]);
    assert_eq!(entries(&icons), vec![stored]);
}

#[test]
fn import_png_reencodes() {
    let (_root, source, icons) = setup();
    let path = source.join("pixel.png");
    fs::write(&path, PNG).expect("write png");

    let got = import(&path, &icons, false).expect("raster import");
    let stored = match &got.spec {
        IconSpec::Image { file } => file.clone(),
        other => panic!("expected an image spec, got {other:?}"),
    };
    assert!(hashed("pixel", "png", &stored), "unexpected name {stored}");
    assert!(got.removed.is_empty());
    let written = fs::read(icons.join(&stored)).expect("read stored png");
    assert_eq!(&written[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(entries(&icons), vec![stored]);
}

#[test]
fn import_refuses_network_path() {
    let (_root, _source, icons) = setup();
    for name in [
        r"\\server\share\icon.png",
        "//server/share/icon.png",
        r"\/server\share\icon.png",
        r"/\server\share\icon.png",
    ] {
        let err = import(Path::new(name), &icons, false)
            .expect_err(&format!("network path {name} must be refused"));
        assert!(
            err.contains("local path"),
            "expected local-path rejection for {name}, got {err:?}"
        );
    }
    assert!(entries(&icons).is_empty());
}

#[cfg(unix)]
#[test]
fn import_refuses_symlink() {
    use std::os::unix::fs::symlink;

    let (_root, source, icons) = setup();
    let real = source.join("real.svg");
    fs::write(&real, SVG).expect("write real svg");
    let link = source.join("link.svg");
    symlink(&real, &link).expect("create symlink");

    assert!(import(&link, &icons, true).is_err());
    assert!(entries(&icons).is_empty());
}

#[test]
fn import_refuses_oversize_without_reading_it_all() {
    let (_root, source, icons) = setup();
    let big_svg = source.join("big.svg");
    fs::write(&big_svg, vec![b' '; SVG_CAP + 1]).expect("write big svg");
    assert!(import(&big_svg, &icons, true).is_err());

    let big_png = source.join("big.png");
    fs::write(&big_png, vec![0u8; RASTER_CAP as usize + 1]).expect("write big png");
    assert!(import(&big_png, &icons, false).is_err());
    assert!(entries(&icons).is_empty());
}

#[test]
fn import_same_file_twice_reuses_name() {
    let (_root, source, icons) = setup();
    let path = source.join("star.svg");
    fs::write(&path, SVG).expect("write svg");

    let first = import(&path, &icons, true).expect("first import");
    let second = import(&path, &icons, true).expect("second import");
    assert_eq!(first, second);
    assert_eq!(entries(&icons).len(), 1);
}
