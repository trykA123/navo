//! Tests of the icon resolver: name rows beat extensions, folders pair closed with open.

use crate::icons::{MaterialIcon, icon_id};

#[test]
fn name_rows_beat_the_extension() {
    assert_eq!(icon_id("Cargo.toml", false, false), MaterialIcon::Rust);
    assert_eq!(icon_id("deny.toml", false, false), MaterialIcon::Rust);
    assert_eq!(icon_id("Cargo.lock", false, false), MaterialIcon::Lock);
    assert_eq!(icon_id("LICENSE.md", false, false), MaterialIcon::License);
    assert_eq!(icon_id(".gitignore", false, false), MaterialIcon::Git);
}

#[test]
fn extensions_then_the_file_default() {
    assert_eq!(icon_id("view.tsx", false, false), MaterialIcon::ReactTs);
    assert_eq!(icon_id("blob.dat", false, false), MaterialIcon::Hex);
    assert_eq!(icon_id("notes.unknown", false, false), MaterialIcon::File);
    assert_eq!(icon_id(".toml", false, false), MaterialIcon::File);
}

#[test]
fn folders_pair_closed_with_open() {
    assert_eq!(icon_id("SRC", true, false), MaterialIcon::FolderSrc);
    assert_eq!(icon_id("src", true, true), MaterialIcon::FolderSrcOpen);
    assert_eq!(icon_id("crates", true, true), MaterialIcon::FolderLibOpen);
    assert_eq!(icon_id("vendor", true, false), MaterialIcon::Folder);
    assert_eq!(icon_id("vendor", true, true), MaterialIcon::FolderOpen);
}
