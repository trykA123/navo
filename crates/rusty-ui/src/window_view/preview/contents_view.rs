use gpui_kit::base::TestSupportExt;
use gpui_kit::{MouseButton, MouseDownEvent, WeakEntity};
use rusty_cache::ProfileCache;
use rusty_fs::{EntryKind, FsEntry};

use super::browse::Pick;
use super::contents::{Contents, RowKind, Shape};
use super::marks::Marks;
use super::spot::Spot;
use super::summary::heading;
use crate::file_list::kind;
use crate::file_list::row::{cursor_bar, inset_ring};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    Plain,
    Passive,
    Active,
}

impl Mark {
    pub(super) fn of(is_spot: bool, is_browsing: bool) -> Self {
        match (is_spot, is_browsing) {
            (false, _) => Self::Plain,
            (true, false) => Self::Passive,
            (true, true) => Self::Active,
        }
    }

    pub(super) fn paint<E: Styled + ParentElement>(self, row: E, t: &Theme) -> E {
        match self {
            Self::Plain => row,
            Self::Passive => row.shadow(inset_ring(t.colors.line)),
            Self::Active => row.bg(t.surfaces.current).child(cursor_bar(t)),
        }
    }
}

pub(super) fn pick(
    view: &WeakEntity<FerriteWindow>,
    spot: Spot,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |event, window, cx| {
        let _ = view.update(cx, |view, cx| {
            let pick = Pick {
                spot,
                clicks: event.click_count,
            };
            view.pick_spot(pick, window, cx);
        });
    }
}

#[derive(Clone, Copy)]
struct Slot {
    index: usize,
    shape: Shape,
}

fn chevron(t: &Theme, slot: Slot, view: &WeakEntity<FerriteWindow>) -> impl IntoElement + use<> {
    let Slot { index, shape } = slot;
    let glyph = if shape.is_open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let view = view.clone();
    div()
        .id(("preview-chevron", index))
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(INDENT))
        .h_full()
        .when(shape.can_open, |slot| {
            slot.child(icon(glyph, px(CHEVRON), t.colors.muted))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = view.update(cx, |view, cx| view.toggle_contents(index, cx));
                })
        })
}

fn size_of(entry: &FsEntry, folder_bytes: Option<u64>) -> String {
    match entry.kind {
        EntryKind::File => format::size(entry.size),
        EntryKind::Folder | EntryKind::Link => folder_bytes.map(format::size).unwrap_or_default(),
    }
}

struct Cells<'a> {
    entry: &'a FsEntry,
    folder_bytes: Option<u64>,
    is_marked: bool,
    index: usize,
}

fn entry_row(t: &Theme, cells: Cells<'_>) -> Vec<AnyElement> {
    let Cells {
        entry,
        folder_bytes,
        is_marked,
        index,
    } = cells;
    let name = entry.name.to_text();
    let (_, extension) = kind::split_name(&name, entry.kind);
    let (glyph, color) = kind::icon(entry.kind, extension, &t.colors);
    let is_folder = entry.kind == EntryKind::Folder;
    let size = size_of(entry, folder_bytes);
    vec![
        icon(glyph, t.spacing.icon, color).into_any_element(),
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .when(is_folder || is_marked, |name| {
                name.font_weight(FontWeight::SEMIBOLD)
            })
            .when(is_marked, |name| name.text_color(t.colors.mark))
            .child(name)
            .into_any_element(),
        div()
            .id(("preview-node-size", index))
            .aria_label(size.clone())
            .test_support()
            .flex_none()
            .text_size(t.typography.small)
            .text_color(if is_marked {
                t.colors.mark
            } else {
                t.colors.muted
            })
            .when(is_marked, |size| size.font_weight(FontWeight::SEMIBOLD))
            .child(size)
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
    marks: &'a Marks,
    spot_row: Option<usize>,
    is_browsing: bool,
}

fn row_cells(source: &Source<'_>, index: usize, kind: &RowKind<'_>) -> Vec<AnyElement> {
    let entry = match kind {
        RowKind::Entry(entry) => *entry,
        RowKind::More(hidden) => return more_row(source.t, *hidden),
    };
    let folder_bytes = source.contents.location(index).and_then(|location| {
        source.marks.size(location).or_else(|| {
            source
                .cache
                .get(location, entry.modified_ms.unwrap_or_default())
                .map(|profile| profile.totals.bytes)
        })
    });
    let is_marked = source
        .contents
        .place(index)
        .and_then(|place| place.location())
        .is_some_and(|location| source.marks.is_marked(&location));
    entry_row(
        source.t,
        Cells {
            entry,
            folder_bytes,
            is_marked,
            index,
        },
    )
}

fn tree_row(view: &WeakEntity<FerriteWindow>, index: usize, source: &Source<'_>) -> AnyElement {
    let t = source.t;
    let Some((kind, shape)) = source.contents.row(index) else {
        return div().into_any_element();
    };
    let cells = row_cells(source, index, &kind);
    let hover = t.colors.hover;
    let is_spot = source.spot_row == Some(index);
    let row = div()
        .id(("preview-node", index))
        .aria_label(spoken(&kind))
        .aria_selected(is_spot)
        .when(shape.can_open, |row| row.aria_expanded(shape.is_open))
        .test_support()
        .relative()
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
        .cursor_pointer()
        .when_some(source.contents.node_at(index), |row, node| {
            row.on_mouse_down(MouseButton::Left, pick(view, Spot::Tree(node)))
        })
        .child(guides(t, shape.depth))
        .child(chevron(t, Slot { index, shape }, view))
        .children(cells);
    Mark::of(is_spot, source.is_browsing)
        .paint(row, t)
        .into_any_element()
}

fn visible_rows(
    view: &WeakEntity<FerriteWindow>,
    range: std::ops::Range<usize>,
    window: &Window,
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
        marks: &folder.marks,
        spot_row: folder.spot_row(),
        is_browsing: shown.preview.is_browsing(window, cx),
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
            move |range, window, cx| visible_rows(&view, range, window, cx),
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
