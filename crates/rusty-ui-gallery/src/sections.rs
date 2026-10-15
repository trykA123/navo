use std::ops::Range;

use rusty_ui::prelude::*;
use rusty_ui::primitives::{
    Button, ButtonVariant, IconButton, IconName, Kbd, ListScroll, Splitter, SplitterState, Tab,
    Tabs, Tooltip, VirtualList, icon,
};
use rusty_ui::tokens::{self, Theme};

const ROWS: usize = 100_000;
const CURSOR_ROW: usize = 2;
const MARKED_ROW: usize = 4;

fn section(t: &Theme, title: &'static str, body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .child(
            div()
                .text_size(t.typography.tiny)
                .font_weight(FontWeight::BOLD)
                .text_color(t.colors.muted)
                .child(title),
        )
        .child(body)
}

fn row_of(t: &Theme) -> Div {
    div().flex().items_center().gap(t.spacing.md)
}

pub fn buttons(
    t: &Theme,
    clicks: usize,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let body = row_of(t)
        .child(
            Button::new("copy", "Copy")
                .variant(ButtonVariant::Primary)
                .on_click(on_click),
        )
        .child(Button::new("move", "Move").tooltip("Move the marked files"))
        .child(Button::new("cancel", "Cancel").variant(ButtonVariant::Ghost))
        .child(Button::new("delete", "Delete").disabled(true))
        .child(
            div()
                .text_color(t.colors.muted)
                .child(format!("Copy clicked {clicks} times")),
        );
    section(t, "BUTTONS: PRIMARY, SECONDARY, GHOST, DISABLED", body)
}

pub fn icon_buttons(t: &Theme) -> Div {
    let body = row_of(t)
        .child(IconButton::new(
            "rail",
            IconName::PanelLeft,
            "Show the navigator",
        ))
        .child(IconButton::new("layers", IconName::Layers, "Panes").selected(true))
        .child(IconButton::new("find", IconName::Search, "Find").disabled(true));
    section(t, "ICON BUTTONS: OFF, ON, DISABLED", body)
}

pub fn keys(t: &Theme) -> Div {
    let body = row_of(t)
        .children(Kbd::new("key-search", "ctrl-k"))
        .children(Kbd::new("key-copy", "f5"))
        .children(Kbd::new("key-set", "alt-1"))
        .children(Kbd::new("key-tab", "ctrl-shift-t"));
    section(t, "KEYS", body)
}

pub fn tooltips(t: &Theme, window: &mut Window, cx: &mut App) -> Div {
    let pinned = Tooltip::new("Copy path").keys("ctrl-c").build(window, cx);
    let body = row_of(t)
        .child(IconButton::new("copy-path", IconName::Copy, "Copy path"))
        .child(div().text_color(t.colors.muted).child("hover, or pinned:"))
        .child(pinned);
    section(t, "TOOLTIP", body)
}

pub fn tabs(
    t: &Theme,
    selected: usize,
    on_select: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> Div {
    let tabs = Tabs::new(
        "tabs",
        [
            Tab::new("commander").icon(IconName::Folder),
            Tab::new("core").icon(IconName::Folder),
            Tab::new("Downloads").icon(IconName::Folder),
        ],
    )
    .selected(selected)
    .on_select(on_select);
    let body = div()
        .border_1()
        .border_color(t.colors.line)
        .rounded(t.radii.md)
        .overflow_hidden()
        .child(tabs)
        .child(div().h(t.spacing.row_height * 2.).bg(t.colors.surface));
    section(t, "TABS: ACTIVE AND INACTIVE", body)
}

fn file_row(index: usize, cx: &App) -> AnyElement {
    let t = tokens::current(cx);
    let c = &t.colors;
    let is_marked = index == MARKED_ROW;
    let (glyph, glyph_color) = if index.is_multiple_of(7) {
        (IconName::Folder, c.folder)
    } else {
        (IconName::FileText, c.muted)
    };
    let background = match index {
        CURSOR_ROW => c.selection,
        _ if index % 2 == 1 => c.zebra,
        _ => c.background,
    };
    let text = if is_marked { c.mark } else { c.text };
    div()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .size_full()
        .pl(t.spacing.xl)
        .pr(t.spacing.xxl)
        .bg(background)
        .text_color(text)
        .when(is_marked, |row| row.font_weight(FontWeight::SEMIBOLD))
        .child(icon(
            glyph,
            t.typography.body + t.spacing.xxs / 2.,
            glyph_color,
        ))
        .child(div().flex_1().child(format!("file-{index:06}.txt")))
        .child(
            div()
                .w(t.spacing.size_column)
                .flex()
                .justify_end()
                .text_size(t.typography.small)
                .text_color(if is_marked { c.mark } else { c.muted })
                .child(format!("{} KB", index % 997 + 1)),
        )
        .into_any_element()
}

pub fn virtual_list(t: &Theme, scroll: &ListScroll) -> Div {
    let list = VirtualList::new("rows", ROWS, scroll, |range: Range<usize>, _, cx| {
        range.map(|index| file_row(index, cx)).collect()
    })
    .label("100,000 generated files");
    let body = div()
        .flex()
        .flex_col()
        .h(t.spacing.row_height * 12.)
        .border_1()
        .border_color(t.colors.line)
        .rounded(t.radii.md)
        .overflow_hidden()
        .child(list);
    section(t, "VIRTUAL LIST: 100,000 ROWS, CURSOR AND MARKED", body)
}

fn pane(t: &Theme, label: &'static str) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(t.colors.rail)
        .text_color(t.colors.muted)
        .child(label)
}

pub fn splitter(t: &Theme, state: &Entity<SplitterState>) -> Div {
    let body = div()
        .h(t.spacing.row_height * 6.)
        .border_1()
        .border_color(t.colors.line)
        .rounded(t.radii.md)
        .overflow_hidden()
        .child(Splitter::new(
            "split",
            state,
            pane(t, "Left pane"),
            pane(t, "Right pane"),
        ));
    section(t, "SPLITTER: DRAG THE DIVIDER (10 TO 90 %)", body)
}
