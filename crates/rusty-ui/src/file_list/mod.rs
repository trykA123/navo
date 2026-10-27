mod action;
mod column_drop;
mod drop;
mod header;
mod icons;
mod keys;
pub(crate) mod kind;
mod lasso;
mod lift;
mod parts;
mod press;
mod resize;
pub(crate) mod row;
mod rows;
mod sort;
mod watch;
mod widths;

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Bounds, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, Keystroke,
    MouseButton, ParentElement, Pixels, RenderOnce, SharedString, Styled, Window, div,
};
use rusty_core::{Command, Selection, ViewState};
use rusty_fs::FsEntry;

pub use action::{FileListAction, FilterEdit};
pub use drop::{DropEvent, DropPayload, DropSpot, ListEdge};
pub use keys::bind_keys;
pub(crate) use keys::{
    ClearFilter, CopyItems, EditItem, MoveItems, MultiRename, NewFile, NewFolder, OpenRow, Paste,
    RecycleItems, Refresh, RenameItem, UndoLast, ViewItem,
};
pub use parts::ListParts;
pub(crate) use row::filter_bar;
pub use sort::{SortBy, SortKey};
pub use widths::UserWidths;

use crate::primitives::{ListScroll, RowSemantics, VirtualList};
use crate::tokens;

type CommandHandler = Rc<dyn Fn(&Command, &mut Window, &mut App)>;
type ActionHandler = Rc<dyn Fn(&FileListAction, &mut Window, &mut App)>;
type PageRowsHandler = Rc<dyn Fn(&u64, &mut Window, &mut App)>;

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
    cover: Option<crate::modifier_hints::Cover>,
    parts: ListParts,
    drag: drop::Wiring,
    known_width: Option<KnownWidth>,
    user_widths: UserWidths,
}

type KnownWidth = Rc<Cell<Option<Pixels>>>;

impl FileList {
    pub fn new(id: impl Into<gpui_kit::ElementId>, props: FileListProps) -> Self {
        Self {
            id: id.into(),
            props,
            focus: None,
            on_command: None,
            on_list_action: None,
            on_page_rows: None,
            cover: None,
            parts: ListParts::default(),
            drag: drop::Wiring::default(),
            known_width: None,
            user_widths: UserWidths::default(),
        }
    }

    pub fn remember_width(mut self, width: Rc<Cell<Option<Pixels>>>) -> Self {
        self.known_width = Some(width);
        self
    }

    #[must_use]
    pub fn parts(mut self, parts: ListParts) -> Self {
        self.parts = parts;
        self
    }

    #[must_use]
    pub fn column_widths(mut self, widths: UserWidths) -> Self {
        self.user_widths = widths;
        self
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
    followed_scroll: Option<(u64, usize, Pixels)>,
    synced_top: Option<(u64, u64)>,
    page_rows: Option<u64>,
    list_bounds: Option<Bounds<Pixels>>,
    lift: Pixels,
    press: press::Press,
    lasso: lasso::Held,
    drop_spot: Option<DropSpot>,
    hover_queued: bool,
    pressed_name: Option<SharedString>,
    resize: Option<resize::Resize>,
}

struct ListContext {
    props: FileListProps,
    state: Entity<ListState>,
    focus: Option<FocusHandle>,
    on_command: Option<CommandHandler>,
    on_list_action: Option<ActionHandler>,
    parts: ListParts,
    widths: widths::ColumnWidths,
    drag: drop::Wiring,
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
        } else {
            cx.propagate();
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

fn virtual_list(ctx: &Rc<ListContext>, scroll: &ListScroll, row_height: Pixels) -> VirtualList {
    let semantics = ctx.clone();
    let label = format!("{} files", ctx.props.folder);
    VirtualList::new("list", ctx.props.rows.len(), scroll, rows::render_rows(ctx))
        .row_height(row_height)
        .label(label)
        .describe_rows(move |index| row_semantics(&semantics.props, index))
        .when_some(ctx.focus.clone(), |list, focus| list.track_focus(&focus))
}

impl RenderOnce for FileList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let row_height = t.rows.height;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ListState::default());
        let wanted = (self.props.view.scroll, self.props.rows.len());
        let scroll = watch::follow_scroll(&state, wanted, row_height, cx);
        let lift = lift::lift_for(&state, (self.props.view.cursor, self.cover), row_height, cx);
        let len = self.props.rows.len();
        let shown = (self.parts, self.user_widths);
        let (parts, widths) = resize::shown((&state, self.known_width.as_ref()), shown, window, cx);
        let ctx = Rc::new(ListContext {
            props: self.props,
            state: state.clone(),
            focus: self.focus,
            on_command: self.on_command,
            on_list_action: self.on_list_action,
            parts,
            widths,
            drag: self.drag,
        });
        let list = virtual_list(&ctx, &scroll, row_height);
        let watch = |state| watch::Watch { state, row_height };
        let area = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .on_children_prepainted(watch::report_page_rows(
                (ctx.clone(), self.on_page_rows, self.known_width),
                watch(state.clone()),
            ))
            .on_scroll_wheel(watch::report_scroll(&ctx, watch(state)))
            .on_mouse_down(MouseButton::Right, press::background_down(&ctx))
            .overflow_hidden()
            .child(lift::lifted(list, lift));
        let area = drop::background(lasso::layer(area, &ctx, &t, cx), &ctx);
        press::release_on(keys::register(div().id(self.id), &ctx), &ctx)
            .test_support()
            .key_context(keys::CONTEXT)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(t.surfaces.card)
            .text_size(t.typography.body)
            .when_some(ctx.props.filter.clone(), |list, text| {
                list.child(row::filter_bar(&t, text, len))
            })
            .child(header::header(&t, ctx.props.sort, &ctx))
            .child(area)
    }
}
