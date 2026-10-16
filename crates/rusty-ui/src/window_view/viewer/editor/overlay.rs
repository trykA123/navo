use gpui_kit::base::input::{Rope, RopeExt};
use gpui_kit::component::input::EditorState;
use gpui_kit::{Bounds, TextAlign, TextRun, canvas, fill, font, point, size};

use crate::prelude::*;
use crate::tokens::Theme;

pub(super) struct OverlayLook {
    pub bookmarks: Vec<u64>,
    pub line_ends: bool,
    pub dot: Hsla,
    pub glyph: Hsla,
    pub mono: SharedString,
    pub glyph_size: Pixels,
    pub dot_size: Pixels,
    pub inset: Pixels,
}

impl OverlayLook {
    pub(super) fn new(t: &Theme, bookmarks: Vec<u64>, line_ends: bool) -> Self {
        Self {
            bookmarks,
            line_ends,
            dot: t.colors.muted,
            glyph: t.colors.muted,
            mono: t.typography.mono_font.clone(),
            glyph_size: t.typography.tiny,
            dot_size: t.spacing.sm,
            inset: t.spacing.xxs,
        }
    }
}

pub(super) fn line_ending(rope: &Rope, row: usize) -> Option<(usize, &'static str)> {
    let end = rope.line_end_offset(row);
    if rope.char_at(end) != Some('\n') {
        return None;
    }
    match end.checked_sub(1).and_then(|before| rope.char_at(before)) {
        Some('\r') => Some((end - 1, "CRLF")),
        _ => Some((end, "LF")),
    }
}

fn paint_dot(look: &OverlayLook, area: Bounds<Pixels>, line: Bounds<Pixels>, window: &mut Window) {
    let y = line.origin.y + (line.size.height - look.dot_size) / 2.;
    let dot = Bounds::new(
        point(area.origin.x + look.inset, y),
        size(look.dot_size, look.dot_size),
    );
    window.paint_quad(fill(dot, look.dot).corner_radii(look.dot_size / 2.));
}

fn paint_glyph(
    look: &OverlayLook,
    glyph: (&'static str, Bounds<Pixels>),
    window: &mut Window,
    cx: &mut App,
) {
    let (text, at) = glyph;
    let run = TextRun {
        len: text.len(),
        font: font(look.mono.clone()),
        color: look.glyph,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(text.into(), look.glyph_size, &[run], None);
    let origin = point(at.origin.x + look.dot_size, at.origin.y);
    let _ = shaped.paint(origin, at.size.height, TextAlign::Left, None, window, cx);
}

enum Paint {
    Dot(Bounds<Pixels>),
    Glyph(&'static str, Bounds<Pixels>),
}

fn collect(state: &EditorState, look: &OverlayLook) -> Vec<Paint> {
    let Some(rows) = state.visible_row_range() else {
        return Vec::new();
    };
    let rope = state.text();
    let mut paints = Vec::new();
    for row in rows {
        let start = rope.line_start_offset(row);
        let Some(line) = state.range_to_bounds(&(start..start)) else {
            continue;
        };
        if look.bookmarks.binary_search(&(start as u64)).is_ok() {
            paints.push(Paint::Dot(line));
        }
        let ending = line_ending(rope, row).filter(|_| look.line_ends);
        if let Some((end, glyph)) = ending
            && let Some(at) = state.range_to_bounds(&(end..end))
        {
            paints.push(Paint::Glyph(glyph, at));
        }
    }
    paints
}

pub(super) fn overlay(editor: Entity<EditorState>, look: OverlayLook) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |area, (), window, cx| {
            let paints = collect(editor.read(cx), &look);
            for paint in paints {
                match paint {
                    Paint::Dot(line) => paint_dot(&look, area, line, window),
                    Paint::Glyph(glyph, at) => paint_glyph(&look, (glyph, at), window, cx),
                }
            }
        },
    )
    .absolute()
    .size_full()
}
