use std::ops::Range;

use gpui_kit::{FontStyle, StrikethroughStyle, TextRun, UnderlineStyle, font};
use rusty_structured::markdown::{Block, Document, Span, Style};

use crate::prelude::*;
use crate::tokens::Theme;

const HAIRLINE: f32 = 1.;

#[derive(Clone, Copy)]
pub(super) struct RunLook<'a> {
    pub t: &'a Theme,
    pub is_mono: bool,
    pub weight: FontWeight,
    pub color: Hsla,
    pub picked: Option<u32>,
}

pub(super) struct Runs {
    pub text: SharedString,
    pub runs: Vec<TextRun>,
    pub links: Vec<(Range<usize>, u32)>,
}

fn color_of(span: &Span, look: &RunLook<'_>) -> Hsla {
    let style = span.style();
    let c = &look.t.colors;
    if (span.link().is_some() && !style.has(Style::IMAGE)) || style.has(Style::FOOTNOTE) {
        c.accent_text
    } else if style.has(Style::HTML) || style.has(Style::IMAGE) {
        c.muted
    } else {
        look.color
    }
}

fn run(span: &Span, len: usize, look: &RunLook<'_>) -> TextRun {
    let style = span.style();
    let t = look.t;
    let is_mono = look.is_mono || style.has(Style::CODE) || style.has(Style::HTML);
    let mut face = font(if is_mono {
        t.typography.mono_font.clone()
    } else {
        t.typography.ui_font.clone()
    });
    face.weight = if style.has(Style::BOLD) {
        FontWeight::BOLD
    } else {
        look.weight
    };
    if style.has(Style::ITALIC) || style.has(Style::IMAGE) {
        face.style = FontStyle::Italic;
    }
    let is_picked = span.link().is_some() && span.link() == look.picked;
    let color = if is_picked && t.is_contrast {
        t.colors.on_selection
    } else {
        color_of(span, look)
    };
    let background = if is_picked {
        Some(t.colors.selection)
    } else if style.has(Style::CODE) && !look.is_mono {
        Some(t.colors.hover)
    } else {
        None
    };
    TextRun {
        len,
        font: face,
        color,
        background_color: background,
        underline: span.link().map(|_| UnderlineStyle {
            thickness: px(HAIRLINE),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: style.has(Style::STRIKE).then_some(StrikethroughStyle {
            thickness: px(HAIRLINE),
            color: Some(color),
        }),
    }
}

pub(super) fn runs(doc: &Document, block: &Block, cell: Option<u8>, look: &RunLook<'_>) -> Runs {
    let text = doc.text(block);
    let mut shown = String::new();
    let mut runs = Vec::new();
    let mut links: Vec<(Range<usize>, u32)> = Vec::new();
    let mut at = 0;
    for span in doc.spans(block) {
        let piece = text.get(at..at + span.len()).unwrap_or_default();
        at += span.len();
        if piece.is_empty() || cell.is_some_and(|cell| cell != span.cell()) {
            continue;
        }
        let start = shown.len();
        shown.push_str(piece);
        runs.push(run(span, piece.len(), look));
        let Some(link) = span.link() else {
            continue;
        };
        match links.last_mut() {
            Some((range, last)) if *last == link && range.end == start => range.end = shown.len(),
            _ => links.push((start..shown.len(), link)),
        }
    }
    Runs {
        text: shown.into(),
        runs,
        links,
    }
}
