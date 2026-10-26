use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role};
use rusty_automotive::Section;
use rusty_l10n::{t, tr};

use super::{percent_text, segment, swatch};
use crate::format;
use crate::prelude::*;
use crate::tokens::{self, Theme};

const SHT_NULL: u32 = 0;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_NOTE: u32 = 7;
const SHT_NOBITS: u32 = 8;
const SHT_DYNSYM: u32 = 11;
const SHT_SYMTAB_SHNDX: u32 = 18;
const BAR_HEIGHT: f32 = 14.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilePart {
    Loaded,
    Dwarf,
    Notes,
    Symbols,
    Other,
}

impl FilePart {
    const ALL: [Self; 5] = [
        Self::Loaded,
        Self::Dwarf,
        Self::Notes,
        Self::Symbols,
        Self::Other,
    ];

    fn text(self) -> &'static str {
        match self {
            Self::Loaded => "space-file-loaded",
            Self::Dwarf => "space-file-dwarf",
            Self::Notes => "space-file-notes",
            Self::Symbols => "space-file-symbols",
            Self::Other => "space-file-other",
        }
    }

    fn colour(self, t: &Theme) -> Hsla {
        match self {
            Self::Loaded => t.colors.accent,
            Self::Dwarf => t.mem.data.strong,
            Self::Notes => t.mem.rodata.strong,
            Self::Symbols => t.mem.bss.strong,
            Self::Other => t.mem.other.strong,
        }
    }

    fn of(section: &Section) -> Self {
        let name = section.name.as_str();
        if section.is_loaded() {
            Self::Loaded
        } else if name.starts_with(".debug_") || name.starts_with(".zdebug_") {
            Self::Dwarf
        } else if section.kind == SHT_NOTE {
            Self::Notes
        } else if matches!(
            section.kind,
            SHT_SYMTAB | SHT_STRTAB | SHT_DYNSYM | SHT_SYMTAB_SHNDX
        ) {
            Self::Symbols
        } else {
            Self::Other
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileBytes {
    bytes: [u64; 5],
}

impl FileBytes {
    #[must_use]
    pub fn of(sections: &[Section]) -> Self {
        let mut out = Self::default();
        for section in sections
            .iter()
            .filter(|section| section.kind != SHT_NULL && section.kind != SHT_NOBITS)
        {
            let slot = &mut out.bytes[FilePart::of(section).index()];
            *slot = slot.saturating_add(section.size);
        }
        out
    }

    #[must_use]
    pub fn bytes(&self, part: FilePart) -> u64 {
        self.bytes[part.index()]
    }

    #[must_use]
    pub fn total(&self) -> u64 {
        self.bytes.iter().sum()
    }

    fn label(&self) -> String {
        let size = |part| format::size(self.bytes(part));
        t!(
            "space-file-bytes-label",
            loaded = size(FilePart::Loaded),
            dwarf = size(FilePart::Dwarf),
            notes = size(FilePart::Notes),
            symbols = size(FilePart::Symbols),
            other = size(FilePart::Other)
        )
    }
}

#[derive(IntoElement)]
pub struct FileBytesBar {
    bytes: FileBytes,
}

impl FileBytesBar {
    pub fn new(sections: &[Section]) -> Self {
        Self::of(FileBytes::of(sections))
    }

    pub fn of(bytes: FileBytes) -> Self {
        Self { bytes }
    }
}

fn chip(t: &Theme, bytes: &FileBytes, part: FilePart) -> Div {
    let value = bytes.bytes(part);
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .child(swatch(t, part.colour(t), false))
        .child(t!(
            "space-file-part",
            name = tr(part.text()),
            size = format::size(value),
            percent = percent_text(value, bytes.total())
        ))
}

impl RenderOnce for FileBytesBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let total = self.bytes.total().max(1) as f64;
        let shown: Vec<FilePart> = FilePart::ALL
            .into_iter()
            .filter(|part| self.bytes.bytes(*part) > 0)
            .collect();
        let bar = shown.iter().fold(
            div()
                .id("space-file-bar")
                .role(Role::ProgressIndicator)
                .aria_label(self.bytes.label())
                .test_support()
                .flex()
                .w_full()
                .h(px(BAR_HEIGHT))
                .overflow_hidden()
                .rounded(t.radii.xs)
                .bg(t.mem.free),
            |bar, part| {
                bar.child(segment(
                    self.bytes.bytes(*part) as f64 / total,
                    part.colour(&t),
                ))
            },
        );
        div()
            .flex()
            .flex_col()
            .gap(t.spacing.xs)
            .child(
                div()
                    .text_size(t.typography.small)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("space-file-bytes-title")),
            )
            .child(bar)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(t.spacing.lg)
                    .gap_y(t.spacing.xs)
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .children(shown.iter().map(|part| chip(&t, &self.bytes, *part))),
            )
    }
}
