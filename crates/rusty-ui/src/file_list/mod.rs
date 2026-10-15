mod header;
mod keys;
pub(crate) mod kind;
pub(crate) mod row;
mod sort;

#[cfg(test)]
mod tests;

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Bounds, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
    Keystroke, MouseButton, ParentElement, Pixels, RenderOnce, ScrollWheelEvent, SharedString,
    Styled, Window, div,
};
use rusty_core::{Command, Selection, ViewState};
use rusty_fs::FsEntry;

pub use keys::bind_keys;
pub use sort::{SortBy, SortKey};

use crate::primitives::{ListScroll, RowSemantics, VirtualList};
use crate::tokens::{self, Theme};
use row::RowLook;

type CommandHandler = Rc<dyn Fn(&Command, &mut Window, &mut App)>;
type ActionHandler = Rc<dyn Fn(&FileListAction, &mut Window, &mut App)>;
type PageRowsHandler = Rc<dyn Fn(&u64, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterEdit {
    Push(char),
    Pop,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileListAction {
    Open(u64),
    Up,
    SwitchPane,
    Operate(rusty_core::OpKind),
    Filter(FilterEdit),
    Rename(u64),
    NewFolder,
    SelectPattern { add: bool },
    Sort(SortBy),
    Edit { row: u64, choose: bool },
    View(u64),
}

#[derive(Clone, Debug)]
pub struct FileListProps {
    pub folder: SharedString,
    pub rows: Rc<[FsEntry]>,
    pub view: ViewState,
    pub marked: Rc<Selection>,
    pub is_active: bool,
    pub filter: Option<SharedString>,
    pub sort: SortKey,
}

#[derive(IntoElement)]
pub struct FileList {
    id: gpui_kit::ElementId,
    props: FileListProps,
    focus: Option<FocusHandle>,
    on_command: Option<CommandHandler>,
    on_list_action: Option<ActionHandler>,
    on_page_rows: Option<PageRowsHandler>,
}

impl FileList {
    pub fn new(id: impl Into<gpui_kit::ElementId>, props: FileListProps) -> Self {
        Self {
            id: id.into(),
            props,
            focus: None,
            on_command: None,
            on_list_action: None,
            on_page_rows: None,
        }
    }

    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn on_command(
        mut self,
        handler: impl Fn(&Command, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_command = Some(Rc::new(handler));
        self
    }

    pub fn on_list_action(
        mut self,
        handler: impl Fn(&FileListAction, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_list_action = Some(Rc::new(handler));
        self
    }

    pub fn on_page_rows(mut self, handler: impl Fn(&u64, &mut Window, &mut App) + 'static) -> Self {
        self.on_page_rows = Some(Rc::new(handler));
        self
    }
}

#[derive(Default)]
struct ListState {
    scroll: ListScroll,
    followed_scroll: Option<(u64, usize)>,
    page_rows: Option<u64>,
}

struct ListContext {
    props: FileListProps,
    focus: Option<FocusHandle>,
    on_command: Option<CommandHandler>,
    on_list_action: Option<ActionHandler>,
}

fn row_number(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

fn row_index(row: u64) -> usize {
    usize::try_from(row).unwrap_or(usize::MAX)
}

impl ListContext {
    fn send(&self, command: Command, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_command {
            handler(&command, window, cx);
        }
    }

    fn act(&self, action: FileListAction, window: &mut Window, cx: &mut App) {
        if let Some(handler) = &self.on_list_action {
            handler(&action, window, cx);
        }
    }

    fn take_focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(focus) = &self.focus {
            window.focus(focus, cx);
        }
    }

    fn click(&self, row: u64, window: &mut Window, cx: &mut App) {
        self.take_focus(window, cx);
        self.send(
            Command::PaneCursorMove {
                to: rusty_core::CursorMove::Row(row),
            },
            window,
            cx,
        );
    }

    fn right_click(&self, row: u64, window: &mut Window, cx: &mut App) {
        self.click(row, window, cx);
        self.send(Command::SelectionToggle, window, cx);
    }

    fn go_up_or_narrow(&self, window: &mut Window, cx: &mut App) {
        let action = if self.props.filter.is_some() {
            FileListAction::Filter(FilterEdit::Pop)
        } else {
            FileListAction::Up
        };
        self.act(action, window, cx);
    }

    fn clear_filter(&self, window: &mut Window, cx: &mut App) {
        if self.props.filter.is_some() {
            self.act(FileListAction::Filter(FilterEdit::Clear), window, cx);
        }
    }

    fn type_key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App) {
        if let Some(ch) = typed_char(&event.keystroke) {
            self.act(FileListAction::Filter(FilterEdit::Push(ch)), window, cx);
        }
    }

    fn mark_key(&self, key: char, window: &mut Window, cx: &mut App) {
        if self.props.filter.is_some() {
            self.act(FileListAction::Filter(FilterEdit::Push(key)), window, cx);
            return;
        }
        match key {
            '+' => self.act(FileListAction::SelectPattern { add: true }, window, cx),
            '-' => self.act(FileListAction::SelectPattern { add: false }, window, cx),
            _ => self.send(Command::SelectionInvert, window, cx),
        }
    }

    fn view_cursor(&self, window: &mut Window, cx: &mut App) {
        let row = self.props.view.cursor;
        if self.props.rows.get(row_index(row)).is_some() {
            self.act(FileListAction::View(row), window, cx);
        }
    }

    fn edit_cursor(&self, choose: bool, window: &mut Window, cx: &mut App) {
        let row = self.props.view.cursor;
        if self.props.rows.get(row_index(row)).is_some() {
            self.act(FileListAction::Edit { row, choose }, window, cx);
        }
    }

    fn rename_cursor(&self, window: &mut Window, cx: &mut App) {
        let cursor = self.props.view.cursor;
        if self.props.rows.get(row_index(cursor)).is_some() {
            self.act(FileListAction::Rename(cursor), window, cx);
        }
    }

    fn open_cursor(&self, window: &mut Window, cx: &mut App) {
        let cursor = self.props.view.cursor;
        if self.props.rows.get(row_index(cursor)).is_some() {
            self.act(FileListAction::Open(cursor), window, cx);
        }
    }
}

pub(crate) fn typed_char(keystroke: &Keystroke) -> Option<char> {
    let modifiers = &keystroke.modifiers;
    let alt_gr = modifiers.control && modifiers.alt;
    let command = modifiers.control || modifiers.alt;
    if modifiers.platform || modifiers.function || (command && !alt_gr) {
        return None;
    }
    let mut chars = keystroke.key_char.as_deref()?.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        return None;
    };
    (!ch.is_control() && !ch.is_whitespace()).then_some(ch)
}

fn row_semantics(props: &FileListProps, index: usize) -> RowSemantics {
    let row = row_number(index);
    RowSemantics {
        label: props
            .rows
            .get(index)
            .map(|entry| kind::spoken(entry).into()),
        is_selected: Some(props.marked.is_marked(row)),
        is_active_descendant: props.is_active && props.view.cursor == row,
    }
}

fn render_row(ctx: &Rc<ListContext>, theme: &Theme, index: usize) -> Option<AnyElement> {
    let entry = ctx.props.rows.get(index)?;
    let row = row_number(index);
    let look = RowLook {
        theme,
        entry,
        index,
        is_cursor: ctx.props.view.cursor == row,
        is_marked: ctx.props.marked.is_marked(row),
        is_active_pane: ctx.props.is_active,
    };
    let left = ctx.clone();
    let right = ctx.clone();
    let element = row::cells(&look)
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            left.click(row, window, cx);
            if event.click_count >= 2 {
                left.act(FileListAction::Open(row), window, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            right.right_click(row, window, cx);
        });
    Some(element.into_any_element())
}

fn render_rows(
    ctx: &Rc<ListContext>,
) -> impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    let ctx = ctx.clone();
    move |range, _, cx| {
        let theme = tokens::current(cx);
        range
            .filter_map(|index| render_row(&ctx, theme, index))
            .collect()
    }
}

fn follow_scroll(
    state: &Entity<ListState>,
    (wanted, len): (u64, usize),
    row_height: Pixels,
    cx: &mut App,
) -> ListScroll {
    state.update(cx, |state, _| {
        if len > 0 && state.followed_scroll != Some((wanted, len)) {
            state.followed_scroll = Some((wanted, len));
            if row_number(state.scroll.top_row(row_height)) != wanted {
                state.scroll.scroll_to_top(row_index(wanted));
            }
        }
        state.scroll.clone()
    })
}

struct Watch {
    state: Entity<ListState>,
    row_height: Pixels,
}

fn report_scroll(
    ctx: &Rc<ListContext>,
    watch: Watch,
) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |_, window, cx| {
        let state = watch.state.read(cx);
        let page = state.page_rows.map_or(1, row_index).max(1);
        let last_top = ctx.props.rows.len().saturating_sub(page);
        let top = row_number(state.scroll.top_row(watch.row_height).min(last_top));
        if top != ctx.props.view.scroll {
            ctx.send(Command::PaneScroll { to: top }, window, cx);
        }
    }
}

fn report_page_rows(
    handler: Option<PageRowsHandler>,
    watch: Watch,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
    move |bounds, window, cx| {
        let Some(list) = bounds.first() else {
            return;
        };
        let rows = (list.size.height / watch.row_height).floor();
        let rows = if rows.is_finite() && rows > 0. {
            rows as u64
        } else {
            0
        };
        let is_new = watch.state.update(cx, |state, _| {
            let is_new = state.page_rows != Some(rows);
            state.page_rows = Some(rows);
            is_new
        });
        if let (true, Some(handler)) = (is_new, handler.clone()) {
            window.defer(cx, move |window, cx| handler(&rows, window, cx));
        }
    }
}

impl RenderOnce for FileList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let row_height = t.spacing.row_height;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ListState::default());
        let wanted = (self.props.view.scroll, self.props.rows.len());
        let scroll = follow_scroll(&state, wanted, row_height, cx);
        let len = self.props.rows.len();
        let label = format!("{} files", self.props.folder);
        let ctx = Rc::new(ListContext {
            props: self.props,
            focus: self.focus,
            on_command: self.on_command,
            on_list_action: self.on_list_action,
        });
        let semantics = ctx.clone();
        let list = VirtualList::new("list", len, &scroll, render_rows(&ctx))
            .label(label)
            .describe_rows(move |index| row_semantics(&semantics.props, index))
            .when_some(ctx.focus.clone(), |list, focus| list.track_focus(&focus));
        let page_watch = Watch {
            state: state.clone(),
            row_height,
        };
        let scroll_watch = Watch { state, row_height };
        let area = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .on_children_prepainted(report_page_rows(self.on_page_rows, page_watch))
            .on_scroll_wheel(report_scroll(&ctx, scroll_watch))
            .child(list);
        keys::register(div().id(self.id), &ctx)
            .test_support()
            .key_context(keys::CONTEXT)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(t.colors.surface)
            .text_size(t.typography.body)
            .when_some(ctx.props.filter.clone(), |list, text| {
                list.child(row::filter_bar(&t, text, len))
            })
            .child(header::header(&t, ctx.props.sort, &ctx))
            .child(area)
    }
}
