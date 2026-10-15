use rusty_core::{Location, ProviderId};
use rusty_fs::archive::{ArchiveEntry, ArchiveFormat, ArchiveListing, Civil, Stamp};
use rusty_ui::prelude::*;
use rusty_ui::window_view::{ArchiveSource, ArchiveView, FileHooks, FontFace, FontView};

const FONT_ENV: &str = "FERRITE_SAMPLE_FONT";
const ARCHIVO: &[u8] = include_bytes!("../../rusty-ui/fonts/Archivo-SemiBold.ttf");

const ENTRIES: [(&str, u64, u8); 14] = [
    ("ferrite/", 0, 9),
    ("ferrite/ferrite.exe", 81_812_480, 9),
    ("ferrite/README.md", 6_214, 9),
    ("ferrite/LICENSE", 1_069, 2),
    ("ferrite/themes/", 0, 7),
    ("ferrite/themes/ferrite-dark.json", 4_382, 7),
    ("ferrite/themes/ferrite-light.json", 4_377, 7),
    ("ferrite/fonts/", 0, 3),
    ("ferrite/fonts/Archivo-Regular.ttf", 146_520, 3),
    ("ferrite/fonts/JetBrainsMono-Regular.ttf", 273_900, 3),
    ("ferrite/plugins/sample/plugin.toml", 412, 11),
    ("ferrite/plugins/sample/main.wasm", 1_904_112, 11),
    ("ferrite/keys/signing.pem", 3_272, 12),
    ("ferrite/CHANGELOG.md", 18_406, 14),
];

fn entry((name, size, day): (&str, u64, u8)) -> ArchiveEntry {
    ArchiveEntry {
        name: name.to_owned(),
        size,
        packed: Some(size * 2 / 5),
        modified: Some(Stamp::Local(Civil {
            year: 2026,
            month: 10,
            day,
            hour: 9 + day % 8,
            minute: day * 4,
        })),
        is_folder: name.ends_with('/'),
        is_encrypted: name.ends_with(".pem"),
    }
}

pub fn archive_view(cx: &mut App) -> Entity<ArchiveView> {
    let listing = ArchiveListing {
        format: ArchiveFormat::Zip,
        entries: ENTRIES.into_iter().map(entry).collect(),
    };
    let source = ArchiveSource {
        location: Location::provider_root(ProviderId::local()),
        hooks: FileHooks::none(),
    };
    cx.new(|cx| ArchiveView::new("ferrite-0.0.1-windows.zip", source, listing, cx))
}

pub fn font_view(cx: &mut App) -> Option<Entity<FontView>> {
    let bytes = std::env::var_os(FONT_ENV)
        .and_then(|path| std::fs::read(path).ok())
        .unwrap_or_else(|| ARCHIVO.to_vec());
    let face = FontFace::parse(bytes).ok()?;
    let title = format!("{}.ttf", face.family.replace(' ', ""));
    Some(cx.new(|cx| FontView::new(title, face, cx)))
}
