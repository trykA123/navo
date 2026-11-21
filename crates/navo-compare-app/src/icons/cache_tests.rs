//! Tests of the icon pack: all 50 variants upload at both HiDPI buckets, and the chrome's
//! monochrome icons are in the bundle `gpui-kit-assets` actually embeds.

use std::collections::HashSet;
use std::sync::Arc;

use gpui_kit::TestAppContext;

use crate::icons::MaterialIcon;
use crate::icons::cache::{BUCKETS, COUNT, IconCache, chrome_icons_bundled, stem_of, svg_of};

/// Every variant in declaration order, paired with the stem the pack must hold for it.
const PAIRS: [(MaterialIcon, &str); COUNT] = [
    (MaterialIcon::C, "c"),
    (MaterialIcon::Console, "console"),
    (MaterialIcon::Css, "css"),
    (MaterialIcon::Database, "database"),
    (MaterialIcon::Dll, "dll"),
    (MaterialIcon::Docker, "docker"),
    (MaterialIcon::Document, "document"),
    (MaterialIcon::Exe, "exe"),
    (MaterialIcon::File, "file"),
    (MaterialIcon::Folder, "folder"),
    (MaterialIcon::FolderConfig, "folder-config"),
    (MaterialIcon::FolderConfigOpen, "folder-config-open"),
    (MaterialIcon::FolderDocs, "folder-docs"),
    (MaterialIcon::FolderDocsOpen, "folder-docs-open"),
    (MaterialIcon::FolderGithub, "folder-github"),
    (MaterialIcon::FolderGithubOpen, "folder-github-open"),
    (MaterialIcon::FolderLib, "folder-lib"),
    (MaterialIcon::FolderLibOpen, "folder-lib-open"),
    (MaterialIcon::FolderOpen, "folder-open"),
    (MaterialIcon::FolderResource, "folder-resource"),
    (MaterialIcon::FolderResourceOpen, "folder-resource-open"),
    (MaterialIcon::FolderScripts, "folder-scripts"),
    (MaterialIcon::FolderScriptsOpen, "folder-scripts-open"),
    (MaterialIcon::FolderSrc, "folder-src"),
    (MaterialIcon::FolderSrcOpen, "folder-src-open"),
    (MaterialIcon::FolderTest, "folder-test"),
    (MaterialIcon::FolderTestOpen, "folder-test-open"),
    (MaterialIcon::Freemarker, "freemarker"),
    (MaterialIcon::Git, "git"),
    (MaterialIcon::Go, "go"),
    (MaterialIcon::H, "h"),
    (MaterialIcon::Hex, "hex"),
    (MaterialIcon::Html, "html"),
    (MaterialIcon::Image, "image"),
    (MaterialIcon::Javascript, "javascript"),
    (MaterialIcon::Json, "json"),
    (MaterialIcon::License, "license"),
    (MaterialIcon::Lock, "lock"),
    (MaterialIcon::Log, "log"),
    (MaterialIcon::Markdown, "markdown"),
    (MaterialIcon::Powershell, "powershell"),
    (MaterialIcon::Python, "python"),
    (MaterialIcon::ReactTs, "react_ts"),
    (MaterialIcon::Readme, "readme"),
    (MaterialIcon::Rust, "rust"),
    (MaterialIcon::Svg, "svg"),
    (MaterialIcon::Toml, "toml"),
    (MaterialIcon::Typescript, "typescript"),
    (MaterialIcon::Yaml, "yaml"),
    (MaterialIcon::Zip, "zip"),
];

#[test]
fn every_variant_is_packed_under_its_stem() {
    for (icon, stem) in PAIRS {
        assert_eq!(
            stem_of(icon),
            Some(stem),
            "declaration order must match the pack"
        );
        let svg = svg_of(icon).unwrap_or_else(|| panic!("{stem} is missing from the pack"));
        assert!(svg.starts_with(b"<svg"), "{stem} must hold an SVG document");
    }
}

#[gpui_kit::test]
fn every_variant_uploads_at_both_buckets(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for (icon, stem) in PAIRS {
            for side in BUCKETS {
                let image = IconCache::image(icon, side, cx)
                    .unwrap_or_else(|| panic!("{stem} did not upload at {side} px"));
                let size = image.size(0);
                assert!(
                    size.width.0 > 0 && size.height.0 > 0,
                    "{stem} at {side} px must have pixels"
                );
                assert_eq!(
                    (size.width.0, size.height.0),
                    (side as i32, side as i32),
                    "{stem} must upload at the bucket size"
                );
            }
        }
    });
}

#[gpui_kit::test]
fn an_upload_is_cached_and_keeps_its_multi_colour_fills(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let first = IconCache::image(MaterialIcon::Docker, 32, cx).expect("docker");
        let second = IconCache::image(MaterialIcon::Docker, 32, cx).expect("docker again");
        assert!(Arc::ptr_eq(&first, &second), "one upload per bucket");
        let bytes = first.as_bytes(0).expect("docker pixels");
        let colours: HashSet<[u8; 4]> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .copied()
            .collect();
        assert!(
            colours.len() > 1,
            "docker keeps its fills, not one alpha mask"
        );
    });
}

#[test]
fn chrome_monochrome_icons_are_in_the_default_bundle() {
    assert!(
        chrome_icons_bundled(),
        "a chrome action icon is unlisted or absent from default-icons.txt, so it draws an empty button"
    );
}
