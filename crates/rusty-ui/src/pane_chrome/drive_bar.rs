use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px, relative,
};

use super::drives::DriveInfo;
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Theme};

pub type PickHandler = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

const USAGE_HEIGHT: f32 = 2.;
const BAR_HEIGHT: f32 = 30.;
const DIMMED: f32 = 0.55;

#[derive(IntoElement)]
pub struct DriveBar {
    id: ElementId,
    drives: Rc<[DriveInfo]>,
    current: Option<usize>,
    busy: Vec<char>,
    on_pick: Option<PickHandler>,
}

impl DriveBar {
    pub fn new(id: impl Into<ElementId>, drives: Rc<[DriveInfo]>) -> Self {
        Self {
            id: id.into(),
            drives,
            current: None,
            busy: Vec::new(),
            on_pick: None,
        }
    }

    pub fn current(mut self, index: Option<usize>) -> Self {
        self.current = index;
        self
    }

    pub fn busy(mut self, letters: Vec<char>) -> Self {
        self.busy = letters;
        self
    }

    pub fn on_pick(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_pick = Some(Rc::new(handler));
        self
    }
}

pub(super) fn usage_bar(t: &Theme, drive: &DriveInfo) -> Div {
    let c = &t.colors;
    let fill = if drive.is_nearly_full() {
        c.warn
    } else {
        c.accent
    };
    div()
        .h(px(USAGE_HEIGHT))
        .w_full()
        .rounded(px(1.))
        .bg(c.line)
        .when_some(drive.used(), |track, used| {
            track.child(div().h_full().w(relative(used)).rounded(px(1.)).bg(fill))
        })
}

pub(super) fn status_value(drive: &DriveInfo, is_busy: bool) -> &'static str {
    match (is_busy, drive.connected) {
        (true, _) => "reconnecting",
        (false, true) => "connected",
        (false, false) => "disconnected",
    }
}

fn face(t: &Theme, glyph: impl IntoElement, name: SharedString) -> Div {
    div()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .child(glyph)
        .child(name)
}

struct ButtonProps<'a> {
    theme: &'a Theme,
    index: usize,
    is_current: bool,
    is_busy: bool,
    on_pick: Option<PickHandler>,
}

fn drive_button(drive: &DriveInfo, props: ButtonProps) -> AnyElement {
    let t = props.theme;
    let c = &t.colors;
    let name = drive.name();
    let tip: SharedString = drive.tooltip().into();
    let glyph = if props.is_busy {
        IconName::RefreshCw
    } else {
        drive.icon.glyph()
    };
    let color = if props.is_current {
        c.accent_text
    } else {
        c.muted
    };
    let index = props.index;
    div()
        .id(("drive", index))
        .role(Role::Button)
        .aria_label(name.clone())
        .aria_selected(props.is_current)
        .aria_value(status_value(drive, props.is_busy))
        .test_support()
        .flex()
        .flex_col()
        .flex_none()
        .justify_center()
        .gap(px(1.))
        .h(px(BAR_HEIGHT - 6.))
        .px(t.spacing.sm)
        .rounded(t.radii.xs)
        .cursor_pointer()
        .text_color(if props.is_current { c.text } else { c.muted })
        .when(props.is_current, |d| {
            d.bg(c.selection).font_weight(FontWeight::SEMIBOLD)
        })
        .when(!props.is_current, |d| d.hover(|style| style.bg(c.hover)))
        .when(!drive.connected || props.is_busy, |d| d.opacity(DIMMED))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .when_some(props.on_pick, |d, handler| {
            d.on_click(move |_, window, cx| handler(&index, window, cx))
        })
        .child(face(t, icon(glyph, px(13.), color), name))
        .child(usage_bar(t, drive))
        .into_any_element()
}

impl RenderOnce for DriveBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let buttons: Vec<AnyElement> = self
            .drives
            .iter()
            .enumerate()
            .map(|(index, drive)| {
                let props = ButtonProps {
                    theme: t,
                    index,
                    is_current: self.current == Some(index),
                    is_busy: drive.letter.is_some_and(|l| self.busy.contains(&l)),
                    on_pick: self.on_pick.clone(),
                };
                drive_button(drive, props)
            })
            .collect();
        div()
            .id(self.id)
            .role(Role::Toolbar)
            .aria_label("Drives")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.xxs)
            .h(px(BAR_HEIGHT))
            .px(t.spacing.sm)
            .min_w_0()
            .overflow_x_scroll()
            .text_size(t.typography.small)
            .children(buttons)
    }
}
