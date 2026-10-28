mod entries;
mod rows;
mod text;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{FocusHandle, KeyDownEvent, RenderOnce, Role};
use rusty_core::retention::RetentionId;
use rusty_l10n::tr;

pub use entries::{Day, DayHead, Entry, MAX_ROWS, Model, PAGE_ROWS};
pub use text::{folder_of, name_of, summary};

use crate::prelude::*;
use crate::primitives::{ListScroll, RowSemantics, VirtualList};
use crate::side_sheet::{SheetFrame, SheetState, SideSheet};
use crate::tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Move(bool),
    Select(usize),
    Restore,
    Delete,
    Close,
}

pub type Send = Rc<dyn Fn(Event, &mut Window, &mut App)>;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Phase {
    Loading,
    Failed(SharedString),
    Ready,
}

pub fn event_for_key(key: &str, modifiers: Modifiers) -> Option<Event> {
    if modifiers.modified() {
        return None;
    }
    match key {
        "up" => Some(Event::Move(false)),
        "down" => Some(Event::Move(true)),
        "enter" => Some(Event::Restore),
        "delete" => Some(Event::Delete),
        "escape" => Some(Event::Close),
        _ => None,
    }
}

#[derive(IntoElement)]
pub struct RecentlyDeletedBody {
    model: Rc<Model>,
    phase: Phase,
    cursor: usize,
    working: Rc<Vec<RetentionId>>,
    now_ms: i64,
    focus: Option<FocusHandle>,
    scroll: ListScroll,
    on_event: Option<Send>,
}

impl RecentlyDeletedBody {
    pub fn new(model: &Rc<Model>, phase: Phase, scroll: &ListScroll) -> Self {
        Self {
            model: model.clone(),
            phase,
            cursor: 0,
            working: Rc::default(),
            now_ms: 0,
            focus: None,
            scroll: scroll.clone(),
            on_event: None,
        }
    }

    #[must_use]
    pub fn cursor(mut self, cursor: usize) -> Self {
        self.cursor = cursor;
        self
    }

    #[must_use]
    pub fn working(mut self, working: &Rc<Vec<RetentionId>>) -> Self {
        self.working = working.clone();
        self
    }

    #[must_use]
    pub fn now_ms(mut self, now_ms: i64) -> Self {
        self.now_ms = now_ms;
        self
    }

    #[must_use]
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    #[must_use]
    pub fn on_event(mut self, handler: impl Fn(Event, &mut Window, &mut App) + 'static) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }

    fn list(&self, t: &tokens::Theme) -> VirtualList {
        let model = self.model.clone();
        let described = self.model.clone();
        let now_ms = self.now_ms;
        let cursor = self.cursor;
        let frame = Rc::new(rows::Frame {
            cursor,
            now_ms,
            working: self.working.clone(),
            send: self.on_event.clone(),
        });
        let height = rows::height(t);
        VirtualList::new("recently-deleted-list", model.len(), &self.scroll, {
            move |range, _, cx| rows::slice(tokens::current(cx), range, (&model, &frame))
        })
        .row_height(height)
        .label(tr("recently-deleted-title"))
        .describe_rows(move |at| RowSemantics {
            label: described
                .row_at(at)
                .map(|row| text::spoken(row, now_ms).into()),
            is_selected: described.row_at(at).map(|_| at == cursor),
            is_active_descendant: at == cursor,
        })
    }
}

fn message(t: &tokens::Theme, title: Option<String>, body: String) -> impl IntoElement {
    let spoken = title.clone().unwrap_or_else(|| body.clone());
    div()
        .id("recently-deleted-message")
        .role(Role::Status)
        .aria_label(spoken)
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.xs)
        .px(t.spacing.xl)
        .py(t.spacing.xxl)
        .children(title.map(|title| div().font_weight(FontWeight::SEMIBOLD).child(title)))
        .child(div().text_color(t.colors.muted).child(body))
}

impl RenderOnce for RecentlyDeletedBody {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let content = match &self.phase {
            Phase::Loading => message(&t, None, tr("recently-deleted-loading")).into_any_element(),
            Phase::Failed(reason) => message(&t, None, reason.to_string()).into_any_element(),
            Phase::Ready if self.model.is_empty() => message(
                &t,
                Some(tr("recently-deleted-empty-title")),
                tr("recently-deleted-empty-body"),
            )
            .into_any_element(),
            Phase::Ready => self.list(&t).into_any_element(),
        };
        let send = self.on_event.clone();
        div()
            .id("recently-deleted-body")
            .role(Role::Group)
            .aria_label(tr("recently-deleted-title"))
            .test_support()
            .when_some(self.focus.as_ref(), |root, focus| root.track_focus(focus))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let Some(found) = event_for_key(key, event.keystroke.modifiers) else {
                    return;
                };
                cx.stop_propagation();
                if let Some(send) = &send {
                    send(found, window, cx);
                }
            })
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(content)
    }
}

#[derive(IntoElement)]
struct Title {
    sub: SharedString,
}

impl RenderOnce for Title {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .truncate()
                    .font_family(t.typography.brand_font.clone())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr("recently-deleted-title")),
            )
            .child(
                div()
                    .truncate()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(self.sub),
            )
    }
}

pub struct SheetParts {
    pub subtitle: SharedString,
    pub status: Option<SharedString>,
    pub has_rows: bool,
}

pub fn sheet(
    state: &Entity<SheetState>,
    frame: SheetFrame,
    body: RecentlyDeletedBody,
    parts: SheetParts,
) -> SideSheet {
    let close = body.on_event.clone();
    let sheet = SideSheet::new("recently-deleted-sheet", state, frame)
        .label(tr("recently-deleted-title"))
        .header(Title {
            sub: parts.subtitle,
        })
        .close("escape", move |window, cx| {
            if let Some(send) = &close {
                send(Event::Close, window, cx);
            }
        })
        .body(body);
    let sheet = if parts.has_rows {
        sheet
            .hint(&["↑", "↓"], "")
            .hint(&["enter"], tr("recently-deleted-restore"))
            .hint(&["delete"], tr("recently-deleted-delete"))
    } else {
        sheet
    };
    match parts.status {
        Some(status) => sheet.status(status),
        None => sheet,
    }
}
