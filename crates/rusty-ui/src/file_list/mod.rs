mod action;
mod drop;
mod header;
mod icons;
mod keys;
pub(crate) mod kind;
mod lasso;
mod lift;
mod parts;
mod press;
pub(crate) mod row;
mod rows;
mod sort;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Bounds, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, Keystroke,
    MouseButton, ParentElement, Pixels, RenderOnce, ScrollWheelEvent, SharedString, Styled, Window,
    div,
};
use rusty_core::{Command, Selection, ViewState};
use rusty_fs::FsEntry;

pub use action::{FileListAction, FilterEdit};
pub use drop::{DropEvent, DropPayload, DropSpot};
pub use keys::bind_keys;
pub(crate) use keys::{
    ClearFilter, CopyItems, EditItem, MoveItems, MultiRename, NewFile, NewFolder, OpenRow, Paste,
    RecycleItems, Refresh, RenameItem, UndoLast, ViewItem,
};
pub use parts::ListParts;
pub(crate) use row::filter_bar;
pub use sort::{SortBy, SortKey};

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
            cover: None,
            parts: ListParts::default(),
            drag: drop::Wiring::default(),
        }
    }

    #[must_use]
    pub fn parts(mut self, parts: ListParts) -> Self {
        self.parts = parts;
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
    page_rows: Option<u64>,
    list_bounds: Option<Bounds<Pixels>>,
    lift: Pixels,
    press: press::Press,
    lasso: lasso::Held,
    drop_spot: Option<DropSpot>,
    hover_queued: bool,
    pressed_name: Option<SharedString>,
}

struct ListContext {
    props: FileListProps,
    state: Entity<ListState>,
    focus: Option<FocusHandle>,
    on_command: Option<CommandHandler>,
    on_list_action: Option<ActionHandler>,
    parts: ListParts,
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

fn follow_scroll(
    state: &Entity<ListState>,
    (wanted, len): (u64, usize),
    row_height: Pixels,
    cx: &mut App,
) -> ListScroll {
    state.update(cx, |state, _| {
        let key = (wanted, len, row_height);
        if len > 0 && state.followed_scroll != Some(key) {
            state.followed_scroll = Some(key);
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
            state.list_bounds = Some(lift::unshifted(*list, state.lift));
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
        let row_height = t.rows.height;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ListState::default());
        let wanted = (self.props.view.scroll, self.props.rows.len());
        let scroll = follow_scroll(&state, wanted, row_height, cx);
        let lift = lift::lift_for(&state, (self.props.view.cursor, self.cover), row_height, cx);
        let len = self.props.rows.len();
        let label = format!("{} files", self.props.folder);
        let ctx = Rc::new(ListContext {
            props: self.props,
            state: state.clone(),
            focus: self.focus,
            on_command: self.on_command,
            on_list_action: self.on_list_action,
            parts: self.parts,
            drag: self.drag,
        });
        let semantics = ctx.clone();
        let list = VirtualList::new("list", len, &scroll, rows::render_rows(&ctx))
            .row_height(row_height)
            .label(label)
            .describe_rows(move |index| row_semantics(&semantics.props, index))
            .when_some(ctx.focus.clone(), |list, focus| list.track_focus(&focus));
        let watch = |state| Watch { state, row_height };
        let area = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .on_children_prepainted(report_page_rows(self.on_page_rows, watch(state.clone())))
            .on_scroll_wheel(report_scroll(&ctx, watch(state)))
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
