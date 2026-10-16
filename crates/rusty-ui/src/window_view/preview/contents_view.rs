use gpui_kit::base::TestSupportExt;
use gpui_kit::{ClickEvent, WeakEntity};
use rusty_cache::ProfileCache;
use rusty_fs::{EntryKind, FsEntry};

use super::contents::{Contents, RowKind, Shape};
use super::summary::heading;
use crate::file_list::kind;
use crate::format;
use crate::prelude::*;
use crate::primitives::{IconName, VirtualList, icon};
use crate::tokens::{self, Theme};
use crate::window_view::FerriteWindow;

const INDENT: f32 = 16.;
const CHEVRON: f32 = 11.;
const ROW_HEIGHT: f32 = 24.;

fn guides(t: &Theme, depth: u16) -> impl IntoElement + use<> {
    let line = t.colors.line;
    div()
        .flex()
        .flex_none()
        .h_full()
        .children((0..depth).map(move |_| {
            div().w(px(INDENT)).h_full().flex_none().child(
                div()
                    .ml(px(INDENT / 2.))
                    .h_full()
                    .border_l_1()
                    .border_color(line),
            )
        }))
}

fn chevron(t: &Theme, shape: Shape) -> impl IntoElement + use<> {
    let glyph = if shape.is_open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(INDENT))
        .h_full()
        .when(shape.can_open, |slot| {
            slot.child(icon(glyph, px(CHEVRON), t.colors.muted))
        })
}

fn size_of(entry: &FsEntry, folder_bytes: Option<u64>) -> String {
    match entry.kind {
        EntryKind::File => format::size(entry.size),
        EntryKind::Folder | EntryKind::Link => folder_bytes.map(format::size).unwrap_or_default(),
    }
}

fn entry_row(t: &Theme, entry: &FsEntry, folder_bytes: Option<u64>) -> Vec<AnyElement> {
    let name = entry.name.to_text();
    let (_, extension) = kind::split_name(&name, entry.kind);
    let (glyph, color) = kind::icon(entry.kind, extension, &t.colors);
    let is_folder = entry.kind == EntryKind::Folder;
    vec![
        icon(glyph, t.spacing.icon, color).into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .when(is_folder, |name| name.font_weight(FontWeight::SEMIBOLD))
            .child(name)
            .into_any_element(),
        div()
            .flex_none()
            .text_size(t.typography.small)
            .text_color(t.colors.muted)
            .child(size_of(entry, folder_bytes))
            .into_any_element(),
    ]
}

fn more_row(t: &Theme, hidden: usize) -> Vec<AnyElement> {
    vec![
        div()
            .flex_1()
            .italic()
            .text_color(t.colors.muted)
            .child(format!("{} more", format::bytes(hidden as u64)))
            .into_any_element(),
    ]
}

fn spoken(kind: &RowKind<'_>) -> String {
    match kind {
        RowKind::Entry(entry) => entry.name.to_text(),
        RowKind::More(hidden) => format!("{hidden} more"),
    }
}

struct Source<'a> {
    t: &'a Theme,
    contents: &'a Contents,
    cache: &'a ProfileCache,
}

fn tree_row(view: &WeakEntity<FerriteWindow>, index: usize, source: &Source<'_>) -> AnyElement {
    let t = source.t;
    let Some((kind, shape)) = source.contents.row(index) else {
        return div().into_any_element();
    };
    let folder_bytes = match (&kind, source.contents.location(index)) {
        (RowKind::Entry(entry), Some(location)) => source
            .cache
            .get(location, entry.modified_ms.unwrap_or_default())
            .map(|profile| profile.totals.bytes),
        _ => None,
    };
    let cells = match &kind {
        RowKind::Entry(entry) => entry_row(t, entry, folder_bytes),
        RowKind::More(hidden) => more_row(t, *hidden),
    };
    let hover = t.colors.hover;
    let view = view.clone();
    div()
        .id(("preview-node", index))
        .aria_label(spoken(&kind))
        .when(shape.can_open, |row| row.aria_expanded(shape.is_open))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .w_full()
        .h(px(ROW_HEIGHT))
        .pl(t.spacing.xs)
        .pr(t.spacing.md)
        .rounded(t.radii.md)
        .overflow_hidden()
        .whitespace_nowrap()
        .hover(move |row| row.bg(hover))
        .when(shape.can_open, |row| {
            row.cursor_pointer().on_click(move |_: &ClickEvent, _, cx| {
                let _ = view.update(cx, |view, cx| view.toggle_contents(index, cx));
            })
        })
        .child(guides(t, shape.depth))
        .child(chevron(t, shape))
        .children(cells)
        .into_any_element()
}

fn visible_rows(
    view: &WeakEntity<FerriteWindow>,
    range: std::ops::Range<usize>,
    cx: &App,
) -> Vec<AnyElement> {
    let Some(this) = view.upgrade() else {
        return Vec::new();
    };
    let shown = this.read(cx);
    let Some(folder) = shown.preview.folder() else {
        return Vec::new();
    };
    let source = Source {
        t: tokens::current(cx),
        contents: &folder.contents,
        cache: shown.browser.profiles(),
    };
    range.map(|index| tree_row(view, index, &source)).collect()
}

impl FerriteWindow {
    pub(super) fn contents_view(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let folder = self.preview.folder()?;
        let contents = &folder.contents;
        let count = if contents.is_read() {
            format::count(contents.top_count() as u64, "item", "items")
        } else {
            "Reading…".to_owned()
        };
        let view = cx.weak_entity();
        let list = VirtualList::new(
            "preview-tree",
            contents.len(),
            &folder.tree_scroll,
            move |range, _, cx| visible_rows(&view, range, cx),
        )
        .row_height(px(ROW_HEIGHT))
        .label("Contents");
        let failure = contents.failure().map(|message| {
            div()
                .py(t.spacing.md)
                .text_color(t.colors.muted)
                .child(message.to_owned())
        });
        Some(
            div()
                .flex()
                .flex_col()
                .size_full()
                .min_h_0()
                .px(t.spacing.xxl)
                .pt(t.spacing.md)
                .child(heading(t, "preview-contents-heading", "Contents", count))
                .children(failure)
                .child(div().flex_1().min_h_0().pt(t.spacing.xs).child(list))
                .into_any_element(),
        )
    }
}
