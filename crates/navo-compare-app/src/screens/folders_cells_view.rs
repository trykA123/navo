//! Cells, gutter and chevron of one side-by-side Folders row (NC131, NC133, NC134).
//! `folders_row` lays the three out; this file only turns a `TreeLine` into their elements.

use chrono::Local;
use gpui_kit::base::{ObservedElement, TestSupportExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};

use super::folders_cells::{GutTag, Newer, SideCell};
use super::folders_model::TreeLine;
use super::folders_row::{KINDS, LineCtx, mark_of, mix_bar};
use super::folders_text::{Meta, items_text, size_text, time_text};
use crate::theme_data::change_syntax::ChangeKind;
use crate::theme_data::palettes::DIMS;

/// A cell shell before its content.
type Cell = ObservedElement<Stateful<Div>>;

const CHEVRON: f32 = 24.; // chevron hit area side, px
const BARE_GUT: f32 = 40.; // gutter width when only names show, px

/// The gutter width for `meta`: 98 px normally, 40 px for names only.
pub(super) fn gut_width(meta: Meta) -> f32 {
    match meta {
        Meta::Bare => BARE_GUT,
        _ => f32::from(DIMS.gut_w),
    }
}

/// The meta text of one cell: items for a folder, size and time for a file, `▲` when newer.
fn meta_text(cell: &SideCell, newer: bool, meta: Meta) -> String {
    let time = || {
        let when = cell.modified_ms.map(|ms| time_text(ms, &Local::now()));
        when.filter(|text| !text.is_empty())
    };
    let text = match (cell.items, meta) {
        (_, Meta::Bare) => String::new(),
        (Some(items), _) => items_text(items),
        (None, Meta::Size) => size_text(cell.size),
        (None, Meta::Full) => [Some(size_text(cell.size)), time()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
    };
    match newer && !text.is_empty() {
        true => format!("▲ {text}"),
        false => text,
    }
}

/// The chevron of a folder line: a 24 px hit area holding a 16 px glyph.
pub(super) fn chevron_view(line: &TreeLine, index: usize, ctx: &LineCtx) -> AnyElement {
    let glyph = if line.expanded { "▾" } else { "▸" };
    let view = ctx.view.clone();
    div()
        .id(("chevron", index))
        .test_support()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CHEVRON))
        .text_size(px(16.))
        .text_color(ctx.m.muted)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            view.update(cx, |this, cx| this.toggle_at(index, cx));
        })
        .child(glyph)
        .into_any_element()
}

/// The flex shell of one cell of line `index`: its test id and layout.
fn cell_base(id: &'static str, index: usize) -> Cell {
    div()
        .id((id, index))
        .test_support()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2()
        .h_full()
}

/// The grey cell of a side that lacks the item: `not in left` / `not in right`.
fn gone_cell(base: Cell, left: bool, ctx: &LineCtx) -> AnyElement {
    let text = if left { "not in left" } else { "not in right" };
    base.bg(ctx.m.layer2)
        .text_color(ctx.m.muted)
        .aria_label(text)
        .child(div().px_2().child(text))
        .into_any_element()
}

/// One side's cell of line `index`; a missing side reads `not in left` / `not in right`.
pub(super) fn cell_view(
    (line, index): (&TreeLine, usize),
    left: bool,
    ctx: &LineCtx,
    cx: &mut App,
) -> AnyElement {
    let (id, cell) = if left {
        ("cell-l", line.left)
    } else {
        ("cell-r", line.right)
    };
    let base = cell_base(id, index);
    let Some(cell) = cell else {
        return gone_cell(base, left, ctx);
    };
    let newer = matches!(
        (line.newer, left),
        (Some(Newer::Left), true) | (Some(Newer::Right), false)
    );
    let meta = meta_text(&cell, newer, ctx.meta);
    let label = match meta.is_empty() {
        true => line.name.clone(),
        false => format!("{} · {meta}", line.name),
    };
    let first = left || line.left.is_none();
    let slot = match (line.folder, first) {
        (true, true) => Some(chevron_view(line, index, ctx)),
        (false, true) if left => Some(div().flex_none().w(px(CHEVRON)).into_any_element()),
        _ => None,
    };
    let indent = if left {
        16. * f32::from(line.depth)
    } else {
        0.
    };
    // An indent spacer, not padding: padding would make the two cells differ in width.
    base.aria_label(SharedString::from(label))
        .child(div().flex_none().w(px(indent)))
        .children(slot)
        .child(mark_of(line, cx))
        .child(name_view(&line.name))
        .when(!meta.is_empty(), |cell| {
            cell.child(meta_view(meta, newer, ctx))
        })
        .into_any_element()
}

/// The truncating name of a cell.
fn name_view(name: &str) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .child(SharedString::from(name.to_string()))
}

/// The mono meta text of a cell; the newer side reads in full text weight.
fn meta_view(meta: String, newer: bool, ctx: &LineCtx) -> impl IntoElement {
    let color = if newer { ctx.m.text } else { ctx.m.muted };
    div()
        .flex_none()
        .pr_2()
        .font_family(ctx.mono.clone())
        .text_color(color)
        .child(SharedString::from(meta))
}

/// The status word, the shown text and the tone of a file's gutter tag.
fn file_gut(tag: GutTag, meta: Meta) -> (&'static str, &'static str, Option<ChangeKind>) {
    let bare = meta == Meta::Bare;
    let pick = |full, short| if bare { short } else { full };
    match tag {
        GutTag::Changed => ("Different", pick("~ changed", "~"), Some(ChangeKind::Chg)),
        GutTag::OnlyLeft => ("Only left", pick("◀ only", "◀"), Some(ChangeKind::Ol)),
        GutTag::OnlyRight => ("Only right", pick("only ▶", "▶"), Some(ChangeKind::Or)),
        GutTag::Conflict => ("Conflict", pick("! both", "!"), Some(ChangeKind::Cfl)),
        GutTag::SameAfterRules => (
            "Same after rules",
            pick("= same*", "="),
            Some(ChangeKind::Same),
        ),
        GutTag::Checking => ("Checking", pick("checking", "…"), None),
    }
}

/// A folder's gutter text: one differing tone names it, a mix says `N inside`; none is empty.
fn folder_gut(line: &TreeLine) -> Option<(String, ChangeKind)> {
    let differing = &line.mix[..4];
    let total: u32 = differing.iter().sum();
    let words = ["changed", "only left", "only right", "conflict"];
    let mut tones = differing.iter().enumerate().filter(|(_, n)| **n > 0);
    match (tones.next(), tones.next()) {
        (None, _) => None,
        (Some((at, n)), None) => Some((format!("{n} {}", words[at]), KINDS[at])),
        _ => Some((format!("{total} inside"), ChangeKind::Chg)),
    }
}

/// The centre Result gutter of line `index`, as wide as the meta mode allows.
pub(super) fn gutter_view((line, index): (&TreeLine, usize), ctx: &LineCtx) -> AnyElement {
    let base = div()
        .id(("gut", index))
        .test_support()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .h_full()
        .w(px(gut_width(ctx.meta)))
        .font_family(ctx.mono.clone())
        .text_size(px(12.));
    let (label, shown, kind) = match (line.folder, line.gut) {
        (true, _) => match folder_gut(line) {
            Some((text, kind)) => (Some(text.clone()), text, Some(kind)),
            None => (None, String::new(), None),
        },
        (false, Some(tag)) => {
            let (label, shown, kind) = file_gut(tag, ctx.meta);
            (Some(label.to_string()), shown.to_string(), kind)
        }
        (false, None) => (None, String::new(), None),
    };
    let color = kind.map_or(ctx.m.muted, |kind| {
        ctx.theme.change.tone(kind, ctx.theme.mode).fg
    });
    let bar = line.folder && !line.expanded && ctx.meta != Meta::Bare && label.is_some();
    let shown = match (line.folder, ctx.meta) {
        (true, Meta::Bare) => shown.split(' ').next().unwrap_or("").to_string(),
        _ => shown,
    };
    base.text_color(color)
        .when_some(label, |gut, label| {
            gut.aria_label(SharedString::from(label))
        })
        .when(
            bar && line.mix.iter().filter(|n| **n > 0).count() > 1,
            |gut| gut.child(mix_bar(line.mix, ctx)),
        )
        .child(SharedString::from(shown))
        .into_any_element()
}
