use gpui_kit::Hsla;
use rusty_fs::EntryKind;

use crate::file_list::kind::icon;
use crate::primitives::IconName;
use crate::tokens::{Colors, Mode, Theme};

type Case = (&'static [&'static str], IconName, fn(&Colors) -> Hsla);

const CASES: &[Case] = &[
    (
        &[
            "rs", "c", "h", "cpp", "hpp", "cs", "go", "java", "kt", "py", "js", "ts", "tsx", "jsx",
            "swift", "zig", "lua", "rb", "php",
        ],
        IconName::FileCode,
        |c| c.kind_code,
    ),
    (
        &["sh", "bash", "zsh", "fish", "ps1", "psm1", "bat", "cmd"],
        IconName::FileTerminal,
        |c| c.kind_code,
    ),
    (
        &["json", "jsonc", "toml", "yml", "yaml", "xml"],
        IconName::FileBraces,
        |c| c.kind_code,
    ),
    (
        &["html", "htm", "css", "scss", "svelte", "vue"],
        IconName::CodeXml,
        |c| c.kind_code,
    ),
    (&["png", "jpg", "svg", "avif"], IconName::FileImage, |c| {
        c.kind_image
    }),
    (&["zip", "7z", "gz", "iso"], IconName::FileArchive, |c| {
        c.kind_archive
    }),
    (&["exe", "msi", "deb"], IconName::SquarePlus, |c| c.kind_app),
    (&["pdf"], IconName::FileText, |c| c.kind_pdf),
    (&["doc", "docx", "odt", "rtf"], IconName::FileText, |c| {
        c.kind_doc
    }),
    (
        &["xls", "xlsx", "ods", "csv", "tsv"],
        IconName::FileSpreadsheet,
        |c| c.kind_sheet,
    ),
    (&["ppt", "pptx", "odp"], IconName::Presentation, |c| {
        c.kind_slides
    }),
    (&["txt", "log"], IconName::FileText, |c| c.muted),
    (&["md", "markdown", "rst"], IconName::BookOpen, |c| c.muted),
    (
        &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
        IconName::FileMusic,
        |c| c.kind_audio,
    ),
    (
        &["mp4", "mkv", "avi", "mov", "webm", "wmv", "m4v"],
        IconName::FilePlay,
        |c| c.kind_video,
    ),
    (&["ttf", "otf", "woff", "woff2"], IconName::FileType, |c| {
        c.muted
    }),
    (
        &["ini", "cfg", "conf", "env", "reg"],
        IconName::FileCog,
        |c| c.muted,
    ),
    (
        &["pem", "crt", "cer", "key", "pfx", "p12"],
        IconName::FileKey,
        |c| c.muted,
    ),
    (
        &["db", "sqlite", "sqlite3", "mdb", "accdb"],
        IconName::Database,
        |c| c.muted,
    ),
    (
        &["elf", "hex", "s19", "s28", "s37", "srec", "mot", "bin"],
        IconName::Cpu,
        |c| c.kind_auto,
    ),
    (
        &["a2l", "dbc", "arxml", "ldf", "cdd", "odx", "pdx"],
        IconName::CircuitBoard,
        |c| c.kind_auto,
    ),
    (
        &["blf", "asc", "trc", "mf4", "mdf", "dat"],
        IconName::Activity,
        |c| c.kind_auto,
    ),
    (&["map"], IconName::Map, |c| c.kind_auto),
];

#[test]
fn every_listed_extension_maps_to_its_group_in_any_case() {
    for mode in [Mode::Light, Mode::Dark] {
        let colors = Theme::new(mode).colors;
        let listed = CASES.iter().flat_map(|(extensions, glyph, tone)| {
            extensions
                .iter()
                .map(move |extension| (*extension, *glyph, *tone))
        });
        for (extension, glyph, tone) in listed {
            for text in [extension.to_owned(), extension.to_uppercase()] {
                let got = icon(EntryKind::File, &text, &colors);
                assert_eq!(got, (glyph, tone(&colors)), "{text}");
            }
        }
    }
}

#[test]
fn unknown_and_missing_extensions_fall_back_to_the_plain_icon() {
    let colors = Theme::new(Mode::Light).colors;
    for extension in ["", "xyz", "pdfx"] {
        assert_eq!(
            icon(EntryKind::File, extension, &colors),
            (IconName::FileText, colors.muted)
        );
    }
}

#[test]
fn folders_and_links_ignore_the_extension() {
    let colors = Theme::new(Mode::Light).colors;
    assert_eq!(
        icon(EntryKind::Folder, "pdf", &colors),
        (IconName::Folder, colors.folder)
    );
    assert_eq!(
        icon(EntryKind::Link, "pdf", &colors),
        (IconName::ArrowRight, colors.muted)
    );
}
