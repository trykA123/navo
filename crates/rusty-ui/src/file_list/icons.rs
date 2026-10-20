use gpui_kit::Hsla;

use crate::primitives::IconName;
use crate::tokens::Colors;

pub(super) type Tone = fn(&Colors) -> Hsla;

pub(super) struct Group {
    pub extensions: &'static [&'static str],
    pub icon: IconName,
    pub tone: Tone,
}

const fn group(extensions: &'static [&'static str], icon: IconName, tone: Tone) -> Group {
    Group {
        extensions,
        icon,
        tone,
    }
}

pub(super) static GROUPS: &[Group] = &[
    group(
        &[
            "rs", "c", "h", "cpp", "hpp", "cs", "go", "java", "kt", "py", "js", "ts", "tsx", "jsx",
            "swift", "zig", "lua", "rb", "php",
        ],
        IconName::FileCode,
        |c| c.kind_code,
    ),
    group(
        &["sh", "bash", "zsh", "fish", "ps1", "psm1", "bat", "cmd"],
        IconName::FileTerminal,
        |c| c.kind_code,
    ),
    group(
        &["json", "jsonc", "toml", "yml", "yaml", "xml"],
        IconName::FileBraces,
        |c| c.kind_code,
    ),
    group(
        &["html", "htm", "css", "scss", "svelte", "vue"],
        IconName::CodeXml,
        |c| c.kind_code,
    ),
    group(
        &[
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "tif", "tiff", "heic", "avif",
        ],
        IconName::FileImage,
        |c| c.kind_image,
    ),
    group(
        &[
            "zip", "7z", "rar", "tar", "gz", "tgz", "xz", "zst", "bz2", "iso", "cab",
        ],
        IconName::FileArchive,
        |c| c.kind_archive,
    ),
    group(
        &["exe", "msi", "appimage", "deb", "rpm"],
        IconName::SquarePlus,
        |c| c.kind_app,
    ),
    group(&["pdf"], IconName::FileText, |c| c.kind_pdf),
    group(&["doc", "docx", "odt", "rtf"], IconName::FileText, |c| {
        c.kind_doc
    }),
    group(
        &["xls", "xlsx", "ods", "csv", "tsv"],
        IconName::FileSpreadsheet,
        |c| c.kind_sheet,
    ),
    group(&["ppt", "pptx", "odp"], IconName::Presentation, |c| {
        c.kind_slides
    }),
    group(&["txt", "log"], IconName::FileText, |c| c.muted),
    group(&["md", "markdown", "rst"], IconName::BookOpen, |c| c.muted),
    group(
        &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
        IconName::FileMusic,
        |c| c.kind_audio,
    ),
    group(
        &["mp4", "mkv", "avi", "mov", "webm", "wmv", "m4v"],
        IconName::FilePlay,
        |c| c.kind_video,
    ),
    group(&["ttf", "otf", "woff", "woff2"], IconName::FileType, |c| {
        c.muted
    }),
    group(
        &["ini", "cfg", "conf", "env", "reg"],
        IconName::FileCog,
        |c| c.muted,
    ),
    group(
        &["pem", "crt", "cer", "key", "pfx", "p12"],
        IconName::FileKey,
        |c| c.muted,
    ),
    group(
        &["db", "sqlite", "sqlite3", "mdb", "accdb"],
        IconName::Database,
        |c| c.muted,
    ),
    group(
        &["elf", "hex", "s19", "s28", "s37", "srec", "mot", "bin"],
        IconName::Cpu,
        |c| c.kind_auto,
    ),
    group(
        &["a2l", "dbc", "arxml", "ldf", "cdd", "odx", "pdx"],
        IconName::CircuitBoard,
        |c| c.kind_auto,
    ),
    group(
        &["blf", "asc", "trc", "mf4", "mdf", "dat"],
        IconName::Activity,
        |c| c.kind_auto,
    ),
    group(&["map"], IconName::Map, |c| c.kind_auto),
];

pub(super) fn lookup(extension: &str) -> Option<&'static Group> {
    GROUPS.iter().find(|group| {
        group
            .extensions
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}
