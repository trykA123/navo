use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{BUNDLED, NOTICE_TXT, Registry, Resolved, registry};
use crate::pack::{self, IconDraft, Pack, SetDraft};
use crate::spec::{BrandColour, IconSpec, Shape, Tone};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let id = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("navo-icons-{prefix}-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn minimal_pack_bytes(set_id: &str, icon_name: &str, body: &[u8]) -> Vec<u8> {
    let draft = SetDraft {
        id: set_id.to_owned(),
        version: "1.0.0".to_owned(),
        licence: "MIT".to_owned(),
        grid: 24,
        icons: vec![IconDraft {
            name: icon_name.to_owned(),
            tags: vec!["sample".to_owned(), "test".to_owned()],
            svg: body.to_vec(),
            brand: None,
        }],
    };
    pack::write(&[draft]).expect("write test pack")
}

fn valid_png_bytes() -> Vec<u8> {
    use image::{ExtendedColorType, ImageEncoder};
    let mut out = Vec::new();
    let rgba = [255_u8, 0, 0, 255];
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(&rgba, 1, 1, ExtendedColorType::Rgba8)
        .expect("encode png");
    out
}

#[test]
fn registry_resolves_bundled_lucide() {
    let reg = registry();
    let dir = TempDir::new("lucide");
    let spec = IconSpec::Set {
        set: "lucide".to_owned(),
        name: "folder".to_owned(),
        tone: Some(Tone::Folder),
    };

    let (resolved, warning) = reg.resolve(&spec, dir.path());
    assert!(warning.is_none(), "unexpected warning: {warning:?}");
    match resolved {
        Resolved::Mono { svg, tone, brand } => {
            assert_eq!(tone, Some(Tone::Folder));
            assert_eq!(brand, None);
            let text = std::str::from_utf8(&svg).expect("utf-8 svg");
            assert!(text.starts_with("<svg"), "expected <svg, got: {text}");
        }
        other => panic!("expected Mono, got {other:?}"),
    }
    assert!(!NOTICE_TXT.is_empty());
}

#[test]
fn registry_resolve_does_not_keep_chunks() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let dir = TempDir::new("trim");
    let spec = IconSpec::Set {
        set: "lucide".to_owned(),
        name: "folder".to_owned(),
        tone: None,
    };

    let (resolved, warning) = reg.resolve(&spec, dir.path());
    assert!(warning.is_none());
    assert!(matches!(resolved, Resolved::Mono { .. }));
    assert!(reg.cached_chunks() >= 1);

    reg.trim();
    assert_eq!(reg.cached_chunks(), 0);
}

fn collect_distinct_chunks(pack: &Pack<'_>, limit: usize) -> Vec<(String, String, u16)> {
    let mut picked = Vec::new();
    let mut seen_chunks = BTreeSet::new();
    for info in pack.sets() {
        for name in pack.names(&info.id) {
            let Some(icon_ref) = pack.find(&info.id, name) else {
                continue;
            };
            if seen_chunks.insert((info.id.clone(), icon_ref.chunk)) {
                picked.push((info.id.clone(), name.to_owned(), icon_ref.chunk));
            }
            if picked.len() == limit {
                return picked;
            }
        }
    }
    picked
}

#[test]
fn registry_lru_keeps_eight_chunks() {
    let pack = Pack::open(BUNDLED).expect("open bundled pack");
    let reg = Registry::new(BUNDLED).expect("open bundled registry");

    let picked = collect_distinct_chunks(&pack, 10);
    assert_eq!(picked.len(), 10, "bundled pack should have >= 10 chunks");

    for (set, name, _) in &picked {
        let body = reg.body(set, name).expect("body should resolve");
        assert!(body.starts_with(b"<svg"));
    }

    assert_eq!(reg.cached_chunks(), 8);
    // Oldest 2 chunks must have been evicted; newest 8 must remain.
    for (set, _, chunk) in &picked[..2] {
        assert!(
            !reg.has_cached_chunk(set, *chunk),
            "expected ({set}, {chunk}) to be evicted"
        );
    }
    for (set, _, chunk) in &picked[2..] {
        assert!(
            reg.has_cached_chunk(set, *chunk),
            "expected ({set}, {chunk}) to stay cached"
        );
    }
}

#[test]
fn registry_skips_download_with_wrong_hash() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let dir = TempDir::new("download");
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>"#;
    let pack_bytes = minimal_pack_bytes("fluent", "home", svg);
    let pack_path = dir.path().join("fluent-1.0.0.nvic");
    fs::write(&pack_path, &pack_bytes).expect("write pack");

    let wrong_hash = "0".repeat(64);
    let manifest_bad = format!(
        "[[set]]\nid = \"fluent\"\nfile = \"fluent-1.0.0.nvic\"\nblake3 = \"{wrong_hash}\"\n"
    );
    fs::write(dir.path().join("installed.toml"), manifest_bad).expect("write manifest");

    let warnings = reg.load_downloaded(dir.path());
    assert_eq!(warnings.len(), 1, "expected 1 warning, got {warnings:?}");
    assert!(reg.body("fluent", "home").is_none());

    let good_hash = blake3::hash(&pack_bytes).to_hex().to_string();
    let manifest_good = format!(
        "[[set]]\nid = \"fluent\"\nfile = \"fluent-1.0.0.nvic\"\nblake3 = \"{good_hash}\"\n"
    );
    fs::write(dir.path().join("installed.toml"), manifest_good).expect("write good manifest");

    let warnings = reg.load_downloaded(dir.path());
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let body = reg.body("fluent", "home").expect("loaded downloaded icon");
    assert_eq!(body.as_ref(), svg);
    assert!(reg.sets().iter().any(|set| set.id == "fluent"));
    let tags = reg.tags("fluent").expect("downloaded tags");
    assert!(tags.contains("sample"));
}

#[test]
fn registry_image_outside_grammar_is_missing() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let parent = TempDir::new("grammar");
    let icons_dir = parent.path().join("icons");
    fs::create_dir_all(&icons_dir).expect("create icons dir");
    fs::write(parent.path().join("secret.png"), valid_png_bytes()).expect("write outside png");
    fs::write(icons_dir.join("bad.png"), valid_png_bytes()).expect("write bad png");

    for bad in ["../secret.png", "a/b-12345678.png", "bad.png"] {
        let spec = IconSpec::Image {
            file: bad.to_owned(),
        };
        let (resolved, warning) = reg.resolve(&spec, &icons_dir);
        assert_eq!(resolved, Resolved::Missing { dashed: false });
        assert!(warning.is_some(), "expected warning for `{bad}`");
    }
}

#[test]
fn registry_revalidates_stored_svg() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let dir = TempDir::new("revalidate-svg");
    let dirty_svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><script>alert(1)</script><path d="M2 2h20v20H2z" fill="#ff0000"/></svg>"##;
    fs::write(dir.path().join("demo-12345678.svg"), dirty_svg).expect("write dirty svg");

    let spec = IconSpec::Image {
        file: "demo-12345678.svg".to_owned(),
    };
    let (resolved, warning) = reg.resolve(&spec, dir.path());
    assert!(warning.is_none(), "unexpected warning: {warning:?}");
    match resolved {
        Resolved::Colour { svg } => {
            let text = std::str::from_utf8(&svg).expect("utf-8 svg");
            assert!(text.starts_with("<svg"));
            assert!(!text.contains("script"), "script was not stripped: {text}");
            assert!(
                text.contains("#ff0000"),
                "colour should be preserved: {text}"
            );
        }
        other => panic!("expected Colour, got {other:?}"),
    }

    let mono_spec = IconSpec::Svg {
        file: "demo-12345678.svg".to_owned(),
        mono: true,
    };
    let (resolved_mono, warning_mono) = reg.resolve(&mono_spec, dir.path());
    assert!(warning_mono.is_none());
    match resolved_mono {
        Resolved::Mono { svg, tone, brand } => {
            assert_eq!(tone, None);
            assert_eq!(brand, None);
            let text = std::str::from_utf8(&svg).expect("utf-8 svg");
            assert!(!text.contains("script"));
        }
        other => panic!("expected Mono, got {other:?}"),
    }
}

#[test]
fn registry_uninstalled_download_set_is_dashed_missing() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let dir = TempDir::new("dashed");
    let spec = IconSpec::Set {
        set: "fluent".to_owned(),
        name: "home".to_owned(),
        tone: None,
    };

    let (resolved, warning) = reg.resolve(&spec, dir.path());
    assert_eq!(resolved, Resolved::Missing { dashed: true });
    assert!(warning.is_some());
}

#[test]
fn registry_resolves_brand_colour_modes() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let dir = TempDir::new("brand");
    let pack = Pack::open(BUNDLED).expect("open bundled");
    let brand_name = pack
        .names("brands")
        .into_iter()
        .find(|name| pack.find("brands", name).and_then(|r| r.brand).is_some())
        .expect("at least one brand with colour");

    let (brand_res, warn) = reg.resolve(
        &IconSpec::Brand {
            name: brand_name.to_owned(),
            colour: BrandColour::Brand,
        },
        dir.path(),
    );
    assert!(warn.is_none());
    assert!(matches!(
        brand_res,
        Resolved::Mono {
            tone: None,
            brand: Some(_),
            ..
        }
    ));

    let (theme_res, warn) = reg.resolve(
        &IconSpec::Brand {
            name: brand_name.to_owned(),
            colour: BrandColour::Theme,
        },
        dir.path(),
    );
    assert!(warn.is_none());
    assert!(matches!(
        theme_res,
        Resolved::Mono {
            tone: None,
            brand: None,
            ..
        }
    ));
}

#[test]
fn registry_resolves_letters_glyph_and_png() {
    let reg = Registry::new(BUNDLED).expect("open bundled");
    let dir = TempDir::new("forms");

    let (letters_res, warn) = reg.resolve(
        &IconSpec::Letters {
            letters: "NV".to_owned(),
            shape: Shape::Circle,
            tone: Tone::Accent,
        },
        dir.path(),
    );
    assert!(warn.is_none());
    assert_eq!(
        letters_res,
        Resolved::Letters {
            letters: "NV".to_owned(),
            shape: Shape::Circle,
            tone: Tone::Accent,
        }
    );

    let (glyph_res, warn) = reg.resolve(
        &IconSpec::Glyph {
            glyph: "folder".to_owned(),
            tone: Tone::Text,
            dot: Some(Tone::Ok),
        },
        dir.path(),
    );
    assert!(warn.is_none());
    assert!(matches!(
        glyph_res,
        Resolved::Glyph {
            tone: Tone::Text,
            dot: Some(Tone::Ok),
            ..
        }
    ));

    fs::write(dir.path().join("photo-abcdef01.png"), valid_png_bytes()).expect("write png");
    let (png_res, warn) = reg.resolve(
        &IconSpec::Image {
            file: "photo-abcdef01.png".to_owned(),
        },
        dir.path(),
    );
    assert!(warn.is_none());
    assert!(matches!(png_res, Resolved::Raster { .. }));
}
