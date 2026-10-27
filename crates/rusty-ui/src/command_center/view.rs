mod chip;
mod footer;
mod motion;
mod place;
mod row;

use std::rc::Rc;

use gpui_kit::base::{Scrollbar, TestSupportExt};
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnchoredPositionMode, AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, Modifiers, ParentElement, Pixels,
    Render, Role, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Subscription,
    WeakEntity, Window, anchored, deferred, div, px,
};
use rusty_l10n::tr;

#[cfg(test)]
pub(crate) use footer::Hint;
pub use footer::HintKeys;
pub(crate) use footer::{hints, result_count};
pub(crate) use place::place;
#[cfg(test)]
pub(crate) use row::highlight_ranges;

use super::query::Query;
use super::results::Row;
use super::selection::Selection;
use crate::commands::ArgKind;
use crate::primitives::{
    IconName, Kbd, ListScroll, RowSemantics, TextFieldState, VirtualList, icon, text_field_state,
};
use crate::tokens::Theme;
use chip::chip;
use footer::FooterLook;
use motion::enter;
pub use place::INPUT_ROW;
use row::{ROW, RowElement, RowLook};

const ROOT: &str = "command-center";
const CONTEXT: &str = "CommandCenter";
const VISIBLE_ROWS: f32 = 12.;
const VIRTUAL_FROM: usize = 64;
const FIELD: f32 = 32.;
const CHIP: f32 = 22.;
const CHAR_WIDTH: f32 = 14.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Argument {
    pub label: SharedString,
    pub kind: ArgKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandCenterProps {
    pub rows: Rc<[Row]>,
    pub selection: Selection,
    pub argument: Option<Argument>,
    pub note: Option<SharedString>,
    pub error: Option<SharedString>,
    pub anchor: Bounds<Pixels>,
    pub key_hints: Option<HintKeys>,
    pub is_leaving: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandCenterEvent {
    QueryChanged(SharedString),
    Moved(usize),
    Run(usize, Modifiers),
    Dismissed,
}

pub struct CommandCenter {
    props: CommandCenterProps,
    field: Entity<TextFieldState>,
    scroll: ScrollHandle,
    list: ListScroll,
    _field: Subscription,
}

impl EventEmitter<CommandCenterEvent> for CommandCenter {}

impl Focusable for CommandCenter {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}

fn placeholder(argument: Option<&Argument>) -> String {
    match argument.map(|argument| argument.kind) {
        None => tr("command-center-placeholder"),
        Some(ArgKind::Path) => tr("command-center-placeholder-path"),
        Some(ArgKind::Text) => tr("command-center-placeholder-text"),
    }
}

impl CommandCenter {
    pub fn new(props: CommandCenterProps, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = text_field_state(placeholder(props.argument.as_ref()), window, cx);
        let subscription = cx.subscribe(&field, |this, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = field.read(cx).value();
                this.scroll_to_selection();
                cx.emit(CommandCenterEvent::QueryChanged(query));
            }
        });
        window.focus(&field.focus_handle(cx), cx);
        Self {
            props,
            field,
            scroll: ScrollHandle::new(),
            list: ListScroll::new(),
            _field: subscription,
        }
    }

    pub fn query(&self, cx: &App) -> SharedString {
        self.field.read(cx).value()
    }

    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = query.to_owned();
        self.field
            .update(cx, |field, cx| field.set_value(text, window, cx));
        cx.notify();
    }

    pub fn set_props(
        &mut self,
        props: CommandCenterProps,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if props.argument != self.props.argument {
            let text = placeholder(props.argument.as_ref());
            self.field
                .update(cx, |field, cx| field.set_placeholder(text, window, cx));
        }
        let has_moved = props.selection != self.props.selection;
        self.props = props;
        if has_moved {
            self.scroll_to_selection();
        }
        cx.notify();
    }

    fn scroll_to_selection(&self) {
        let Some(index) = self.props.selection.index() else {
            return;
        };
        if self.is_virtual() {
            self.list.scroll_to(index);
        } else {
            self.scroll.scroll_to_item(index);
        }
    }

    fn is_virtual(&self) -> bool {
        self.props.rows.len() >= VIRTUAL_FROM
    }

    fn gutter(&self) -> Pixels {
        if self.is_virtual() {
            Scrollbar::width()
        } else {
            px(0.)
        }
    }
}

fn interactive(
    element: RowElement,
    row: &Row,
    (index, is_selected): (usize, bool),
    view: &WeakEntity<CommandCenter>,
) -> RowElement {
    if !row.is_selectable() {
        return element;
    }
    let (moved, run) = (view.clone(), view.clone());
    element
        .when(!is_selected, |element| {
            element.on_mouse_move(move |_, _, cx| {
                let _ = moved.update(cx, |_, cx| cx.emit(CommandCenterEvent::Moved(index)));
            })
        })
        .on_click(move |event: &ClickEvent, _, cx| {
            let modifiers = event.modifiers();
            let _ = run.update(cx, |_, cx| {
                cx.emit(CommandCenterEvent::Run(index, modifiers));
            });
        })
}

struct Paint {
    theme: Theme,
    rows: Rc<[Row]>,
    selected: Option<usize>,
    detail_chars: usize,
    gutter: Pixels,
    view: WeakEntity<CommandCenter>,
}

impl Paint {
    fn rows(&self, range: std::ops::Range<usize>) -> Vec<AnyElement> {
        let look = RowLook {
            theme: &self.theme,
            detail_chars: self.detail_chars,
            gutter: self.gutter,
        };
        self.rows[range.clone()]
            .iter()
            .zip(range)
            .map(|(row, index)| {
                let is_selected = self.selected == Some(index);
                let element = row::render(&look, row, index, is_selected);
                interactive(element, row, (index, is_selected), &self.view).into_any_element()
            })
            .collect()
    }
}

impl CommandCenter {
    fn input_row(&self, t: &Theme) -> Div {
        let input = Input::new(&self.field)
            .id("command-center-input")
            .aria_label(tr("command-center"))
            .appearance(false)
            .h(px(FIELD))
            .text_size(t.typography.body);
        let field = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(t.spacing.md)
            .h(px(FIELD))
            .pl(t.spacing.xl)
            .pr(t.spacing.xs)
            .rounded(t.radii.full)
            .border_1()
            .border_color(t.colors.accent)
            .bg(t.surfaces.field)
            .child(icon(IconName::Search, t.spacing.icon, t.colors.muted))
            .when_some(self.props.argument.as_ref(), |field, argument| {
                field.child(chip(t, argument))
            })
            .child(div().flex_1().min_w_0().child(input))
            .children(self.escape_key());
        div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(INPUT_ROW))
            .p(t.spacing.xs)
            .child(field)
    }

    fn escape_key(&self) -> Option<Kbd> {
        let key = match &self.props.key_hints {
            Some(keys) => keys.key(footer::BACK_COMMAND)?,
            None => "escape",
        };
        Kbd::new("command-center-escape", key)
    }

    fn list(&self, paint: Paint) -> AnyElement {
        let t = paint.theme.clone();
        let len = paint.rows.len();
        let label = tr("command-center-results-label");
        if self.is_virtual() {
            let rows = paint.rows.clone();
            let selected = paint.selected;
            let list = VirtualList::new(
                "command-center-list",
                len,
                &self.list,
                move |range, _, _| paint.rows(range),
            )
            .row_height(px(ROW))
            .label(label)
            .describe_rows(move |index| RowSemantics {
                label: None,
                is_selected: rows[index]
                    .is_selectable()
                    .then_some(selected == Some(index)),
                is_active_descendant: selected == Some(index),
            });
            return div()
                .flex()
                .flex_none()
                .h(px(ROW * VISIBLE_ROWS) + t.spacing.xs * 2.)
                .p(t.spacing.xs)
                .border_t_1()
                .border_color(t.colors.line)
                .child(list)
                .into_any_element();
        }
        div()
            .id("command-center-list")
            .role(Role::ListBox)
            .aria_label(label)
            .test_support()
            .flex()
            .flex_col()
            .flex_shrink(1.)
            .min_h_0()
            .max_h(px(ROW * VISIBLE_ROWS) + t.spacing.xs * 2.)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p(t.spacing.xs)
            .border_t_1()
            .border_color(t.colors.line)
            .children(paint.rows(0..len))
            .into_any_element()
    }

    fn body(&self, paint: Paint, query: &str) -> Option<AnyElement> {
        if !paint.rows.is_empty() {
            return Some(self.list(paint));
        }
        let kind = self.props.argument.as_ref().map(|argument| argument.kind);
        let has_query =
            !query.trim().is_empty() && kind != Some(ArgKind::Text) && self.props.error.is_none();
        let is_path = kind == Some(ArgKind::Path);
        has_query.then(|| footer::empty_message(&paint.theme, query, is_path).into_any_element())
    }

    fn footer(&self, t: &Theme, query: &str) -> Div {
        let props = &self.props;
        let argument = props.argument.as_ref().map(|argument| argument.kind);
        let is_counted = argument.is_some() || !query.trim().is_empty();
        let is_path = Query::parse(query).path.is_some();
        let selected = props.selection.row(&props.rows);
        let keys = props.key_hints.as_ref();
        footer::render(FooterLook {
            theme: t,
            note: props.note.clone(),
            error: props.error.clone(),
            count: (is_counted && !props.rows.is_empty()).then(|| result_count(&props.rows)),
            hints: keys.map(|keys| (hints(selected, argument, is_path), keys)),
        })
    }
}

impl Render for CommandCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::tokens::current(cx).clone();
        let frame = place(self.props.anchor, window.viewport_size());
        let query = self.query(cx).to_string();
        let paint = Paint {
            theme: t.clone(),
            rows: self.props.rows.clone(),
            selected: self.props.selection.index(),
            detail_chars: (frame.width / px(CHAR_WIDTH)) as usize,
            gutter: self.gutter(),
            view: cx.entity().downgrade(),
        };
        let panel = div()
            .id(ROOT)
            .key_context(CONTEXT)
            .role(Role::Dialog)
            .aria_label(tr("command-center"))
            .test_support()
            .occlude()
            .flex()
            .flex_col()
            .w(frame.width)
            .max_h(frame.max_height)
            .overflow_hidden()
            .rounded(t.radii.sheet)
            .border_1()
            .border_color(t.surfaces.card_edge)
            .bg(t.surfaces.sheet)
            .shadow(t.shadows.menu.clone())
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .line_height(t.typography.line_height)
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(CommandCenterEvent::Dismissed)))
            .child(self.input_row(&t))
            .children(self.body(paint, &query))
            .child(self.footer(&t, &query));
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(frame.origin)
                .child(enter(panel, &t.motion, self.props.is_leaving)),
        )
        .with_priority(2)
    }
}
