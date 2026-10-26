use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, BoxShadow, Div, ElementId, FocusHandle, FontWeight, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Pixels, RenderOnce, Role, Stateful,
    StatefulInteractiveElement, Styled, Window, deferred, div, point, px,
};

use rusty_core::Location;

use super::drive_pill::{status_value, usage_bar};
use super::drives::DriveInfo;
use crate::drag::Landing;
use crate::file_list::DropPayload;
use crate::primitives::icon;
use crate::tokens::{self, Theme};

const CONTEXT: &str = "DriveMenu";
const WIDTH: f32 = 400.;
const BAR_WIDTH: f32 = 56.;

gpui_kit::actions!(drive_menu, [MenuUp, MenuDown, MenuOpen, MenuClose]);

pub(super) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "drive_menu");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveMenuEvent {
    Select(usize),
    Open(usize),
    Close,
}

type EventHandler = Rc<dyn Fn(&DriveMenuEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct DriveMenu {
    id: ElementId,
    drives: Rc<[DriveInfo]>,
    selected: usize,
    current: Option<usize>,
    busy: Vec<char>,
    focus: Option<FocusHandle>,
    on_event: Option<EventHandler>,
    landing: Landing<Location>,
}

impl DriveMenu {
    pub fn new(id: impl Into<ElementId>, drives: Rc<[DriveInfo]>, selected: usize) -> Self {
        Self {
            id: id.into(),
            drives,
            selected,
            current: None,
            busy: Vec::new(),
            focus: None,
            on_event: None,
            landing: Landing::default(),
        }
    }

    pub fn current(mut self, index: Option<usize>) -> Self {
        self.current = index;
        self
    }

    #[must_use]
    pub fn busy(mut self, letters: Vec<char>) -> Self {
        self.busy = letters;
        self
    }

    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn on_event(
        mut self,
        handler: impl Fn(&DriveMenuEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }

    pub fn on_drop(
        mut self,
        handler: impl Fn(&Location, &DropPayload, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_drop(handler);
        self
    }

    pub fn on_hover(
        mut self,
        handler: impl Fn(&Location, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.landing.on_hover(handler);
        self
    }

    pub fn into_popover(self, right: Pixels) -> AnyElement {
        let anchored = div()
            .absolute()
            .top_full()
            .right(right)
            .mt(px(4.))
            .child(self);
        deferred(anchored).with_priority(1).into_any_element()
    }
}

#[must_use]
pub fn drive_for_key(drives: &[DriveInfo], key: &str) -> Option<usize> {
    let mut chars = key.chars();
    let (Some(typed), None) = (chars.next(), chars.next()) else {
        return None;
    };
    drives.iter().position(|drive| {
        drive
            .letter
            .is_some_and(|letter| letter.eq_ignore_ascii_case(&typed))
    })
}

fn letter_cell(t: &Theme, letter: String, is_current: bool) -> Div {
    div()
        .w(px(22.))
        .flex_none()
        .font_weight(FontWeight::BOLD)
        .when(is_current, |d| d.text_color(t.colors.accent_text))
        .child(letter)
}

struct RowProps<'a> {
    theme: &'a Theme,
    index: usize,
    is_selected: bool,
    is_current: bool,
    is_busy: bool,
    send: Option<EventHandler>,
    landing: &'a Landing<Location>,
}

fn row(drive: &DriveInfo, props: RowProps) -> AnyElement {
    let (landing, ring) = (props.landing, props.theme.colors.accent);
    let el = row_body(drive, props);
    match drive.connected {
        true => landing
            .accept(el, (drive.root.clone(), ring))
            .into_any_element(),
        false => el.into_any_element(),
    }
}

fn row_body(drive: &DriveInfo, props: RowProps) -> impl InteractiveElement + IntoElement + use<> {
    let t = props.theme;
    let c = &t.colors;
    let index = props.index;
    let letter = drive.letter.map(|l| format!("{l}:")).unwrap_or_default();
    let label = drive.label.clone().unwrap_or_else(|| drive.name().into());
    let (hover, click) = (props.send.clone(), props.send);
    div()
        .id(("drive-row", index))
        .role(Role::MenuItem)
        .aria_label(drive.name())
        .aria_selected(props.is_selected)
        .aria_value(status_value(drive, props.is_busy))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.md)
        .h(px(30.))
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .cursor_pointer()
        .when(props.is_selected, |d| d.bg(c.selection))
        .when(!drive.connected, |d| d.text_color(c.muted))
        .on_hover(move |is_hovered, window, cx| {
            if let (true, Some(send)) = (*is_hovered, &hover) {
                send(&DriveMenuEvent::Select(index), window, cx);
            }
        })
        .when_some(click, |d, send| {
            d.on_click(move |_, window, cx| send(&DriveMenuEvent::Open(index), window, cx))
        })
        .child(icon(drive.icon.glyph(), px(14.), c.muted))
        .child(letter_cell(t, letter, props.is_current))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(
            div()
                .w(px(BAR_WIDTH))
                .flex_none()
                .child(usage_bar(t, drive)),
        )
        .child(
            div()
                .w(px(156.))
                .flex_none()
                .truncate()
                .text_size(t.typography.tiny)
                .text_color(c.muted)
                .child(drive.status()),
        )
}

fn shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: gpui_kit::black().opacity(0.16),
        offset: point(px(0.), px(4.)),
        blur_radius: px(14.),
        spread_radius: px(0.),
        inset: false,
    }]
}

fn send_to(
    handler: Option<&EventHandler>,
    event: DriveMenuEvent,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(handler) = handler {
        handler(&event, window, cx);
    }
}

struct Keys {
    send: Option<EventHandler>,
    selected: usize,
    count: usize,
    drives: Rc<[DriveInfo]>,
}

fn register(root: Stateful<Div>, keys: Keys) -> Stateful<Div> {
    let Keys {
        send,
        selected,
        count,
        drives,
    } = keys;
    let (up, down, open, close, out) = (
        send.clone(),
        send.clone(),
        send.clone(),
        send.clone(),
        send.clone(),
    );
    let last = count.saturating_sub(1);
    root.on_action(move |_: &MenuUp, window, cx| {
        let event = DriveMenuEvent::Select(selected.saturating_sub(1));
        send_to(up.as_ref(), event, window, cx);
    })
    .on_action(move |_: &MenuDown, window, cx| {
        let event = DriveMenuEvent::Select((selected + 1).min(last));
        send_to(down.as_ref(), event, window, cx);
    })
    .on_action(move |_: &MenuOpen, window, cx| {
        send_to(open.as_ref(), DriveMenuEvent::Open(selected), window, cx);
    })
    .on_action(move |_: &MenuClose, window, cx| {
        send_to(close.as_ref(), DriveMenuEvent::Close, window, cx);
    })
    .on_key_down(move |event: &KeyDownEvent, window, cx| {
        if let Some(index) = drive_for_key(&drives, &event.keystroke.key) {
            send_to(send.as_ref(), DriveMenuEvent::Select(index), window, cx);
        }
    })
    .on_mouse_down_out(move |_, window, cx| {
        send_to(out.as_ref(), DriveMenuEvent::Close, window, cx);
    })
}

impl RenderOnce for DriveMenu {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let c = &t.colors;
        let rows: Vec<AnyElement> = self
            .drives
            .iter()
            .enumerate()
            .map(|(index, drive)| {
                let props = RowProps {
                    theme: t,
                    index,
                    is_selected: index == self.selected,
                    is_current: self.current == Some(index),
                    is_busy: drive.letter.is_some_and(|l| self.busy.contains(&l)),
                    send: self.on_event.clone(),
                    landing: &self.landing,
                };
                row(drive, props)
            })
            .collect();
        let keys = Keys {
            send: self.on_event.clone(),
            selected: self.selected,
            count: self.drives.len(),
            drives: self.drives.clone(),
        };
        register(div().id(self.id), keys)
            .key_context(CONTEXT)
            .role(Role::Menu)
            .aria_label("Drives")
            .test_support()
            .when_some(self.focus.as_ref(), |d, focus| d.track_focus(focus))
            .occlude()
            .flex()
            .flex_col()
            .w(px(WIDTH))
            .p(t.spacing.xs)
            .rounded(t.radii.md)
            .border_1()
            .border_color(c.line)
            .bg(c.surface)
            .shadow(shadow())
            .text_color(c.text)
            .text_size(t.typography.small)
            .children(rows)
    }
}
