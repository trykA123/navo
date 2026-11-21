//! Row drawing of the Folders view: tree lines, the folder mix bar and the summary chips.
//! Split from `folders_view` to keep both files short; the view owns the state.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnimationExt, AnyElement, App, Bounds, ClickEvent, ElementId, Entity, InteractiveElement,
    IntoElement, ParentElement, Pixels, SharedString, StatefulInteractiveElement, Styled, canvas,
    div, img, px,
};
use sidewise_ui::browser::CompareFilter;

use super::folders_cells_view::{cell_view, gutter_view};
use super::folders_model::{Mix, TreeLine};
use super::folders_text::Meta;
use super::folders_view::FoldersView;
use crate::icons::{cache::IconCache, icon_id};
use crate::motion;
use crate::theme::SidewiseTheme;
use crate::theme_data::change_syntax::ChangeKind;
use crate::theme_data::palettes::{DIMS, ModeTokens};

const MARK: u32 = 16; // mark side, px
const MIX_W: f32 = 56.; // folder mix bar width, px

/// Everything one line needs to draw: the theme, the cursor, the fade and the owning view.
pub(super) struct LineCtx {
    pub theme: SidewiseTheme,
    pub m: ModeTokens,
    pub cursor: usize,
    pub fade: Option<(String, u64)>,
    pub filter: CompareFilter,
    pub view: Entity<FoldersView>,
    pub cursor_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub meta: Meta,
    pub mono: SharedString,
}

/// The `ChangeKind` of each tone, in `SwTone` order (also the mix-bar order).
pub(super) const KINDS: [ChangeKind; 5] = [
    ChangeKind::Chg,
    ChangeKind::Ol,
    ChangeKind::Or,
    ChangeKind::Cfl,
    ChangeKind::Same,
];

/// A 56 px bar of five segments, each as wide as its share of `mix`.
pub(super) fn mix_bar(mix: Mix, ctx: &LineCtx) -> impl IntoElement {
    let total: u32 = mix.iter().sum();
    let bar = div()
        .flex()
        .h(px(4.))
        .w(px(MIX_W))
        .rounded_full()
        .overflow_hidden();
    bar.children(
        KINDS
            .iter()
            .zip(mix)
            .filter(|(_, n)| *n > 0)
            .map(|(kind, n)| {
                let mark = ctx.theme.change.tone(*kind, ctx.theme.mode).mark;
                div()
                    .h_full()
                    .w(px(MIX_W * n as f32 / total as f32))
                    .bg(mark)
            }),
    )
}

/// The folder or file mark of `line` at the 1x bucket; empty when the raster is missing.
pub(super) fn mark_of(line: &TreeLine, cx: &mut App) -> impl IntoElement {
    let image = IconCache::image(icon_id(&line.name, line.folder, line.expanded), MARK, cx);
    let side = px(MARK as f32);
    div()
        .size(side)
        .flex_none()
        .children(image.map(|image| img(image).size(side)))
}

/// One 30 px line: left cell, centre gutter and right cell; the cursor line has an accent bar.
pub(super) fn line_view(line: &TreeLine, index: usize, ctx: &LineCtx, cx: &mut App) -> AnyElement {
    let tone = line
        .tone
        .map(|tone| ctx.theme.change.tone(KINDS[tone as usize], ctx.theme.mode));
    let view = ctx.view.clone();
    let selected = index == ctx.cursor;
    let at = (line, index);
    let row = div()
        .id(("folder-line", index))
        .test_support()
        .aria_selected(selected)
        .relative()
        .flex()
        .w_full()
        .items_center()
        .h(DIMS.row_h)
        .text_size(DIMS.fs)
        .text_color(tone.map_or(ctx.m.text, |tone| tone.fg))
        .when(selected, |row| row.bg(ctx.m.selected))
        .on_click(move |event, _, cx| {
            view.update(cx, |this, cx| match event.click_count() >= 2 {
                true => this.double_line(index, cx),
                false => this.click_line(index, cx),
            })
        })
        .on_aux_click(menu_click(ctx.view.clone(), index))
        .child(cell_view(at, true, ctx, cx))
        .child(gutter_view(at, ctx))
        .child(cell_view(at, false, ctx, cx))
        .when(selected, |row| row.child(cursor_marks(ctx)));
    fade_in(row, (line, index), ctx, cx)
}

/// The right-click handler of line `index`: select it, then open its menu at the pointer.
fn menu_click(
    view: Entity<FoldersView>,
    index: usize,
) -> impl Fn(&ClickEvent, &mut gpui_kit::Window, &mut App) + 'static {
    move |event, window, cx| {
        if event.is_secondary() {
            let pointer = window.mouse_position();
            view.update(cx, |this, cx| {
                this.menu_at_pointer((index, pointer), window, cx)
            });
        }
    }
}

/// The cursor line's accent bar, with a probe that records where the line is drawn.
fn cursor_marks(ctx: &LineCtx) -> impl IntoElement {
    let cell = ctx.cursor_bounds.clone();
    div()
        .absolute()
        .size_full()
        .child(
            canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, (), _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(px(3.))
                .bg(ctx.m.accent),
        )
}

/// Wraps `row` in the opacity fade when it sits in the subtree that just opened.
fn fade_in(
    row: impl IntoElement,
    (line, index): (&TreeLine, usize),
    ctx: &LineCtx,
    cx: &App,
) -> AnyElement {
    let inside = |(path, n): &(String, u64)| {
        let rest = line.path.strip_prefix(path.as_str())?;
        rest.starts_with('/').then_some(*n)
    };
    let Some(generation) = ctx.fade.as_ref().and_then(inside) else {
        return row.into_any_element();
    };
    let id = ElementId::from((SharedString::from(format!("fade-{generation}")), index));
    match motion::anim(id.clone(), motion::row(cx).as_millis() as u64, cx) {
        Some(anim) => div()
            .child(row)
            .with_animation(id, anim, |el, delta| el.opacity(delta))
            .into_any_element(),
        None => row.into_any_element(),
    }
}

/// One filter chip: a label with its file count; the active filter shows in the accent.
fn chip_view(label: &str, count: u32, filter: CompareFilter, ctx: &LineCtx) -> AnyElement {
    let view = ctx.view.clone();
    let (fg, bg) = match filter == ctx.filter {
        true => (ctx.m.accent, ctx.m.accent_tint),
        false => (ctx.m.muted, ctx.m.layer2),
    };
    div()
        .id(SharedString::from(format!("chip-{label}")))
        .test_support()
        .px_3()
        .h(px(24.))
        .flex()
        .items_center()
        .rounded_full()
        .text_size(DIMS.fs)
        .text_color(fg)
        .bg(bg)
        .on_click(move |_, _, cx| view.update(cx, |this, cx| this.set_filter(filter, cx)))
        .aria_label(SharedString::from(format!("{label} {count}")))
        .child(SharedString::from(format!("{label} {count}")))
        .into_any_element()
}

/// The summary bar: chips for All, Differences, Orphans and Same, counted from `totals`.
pub(super) fn summary_bar(totals: Mix, ctx: &LineCtx) -> impl IntoElement + use<> {
    let [chg, ol, or, cfl, same] = totals;
    let chips = [
        ("All", totals.iter().sum(), CompareFilter::All),
        (
            "Differences",
            chg + ol + or + cfl,
            CompareFilter::Differences,
        ),
        ("Orphans", ol + or, CompareFilter::Orphans),
        ("Same", same, CompareFilter::Same),
    ];
    let chips = chips.map(|(label, count, filter)| chip_view(label, count, filter, ctx));
    div().flex().gap_2().p_2().children(chips)
}
