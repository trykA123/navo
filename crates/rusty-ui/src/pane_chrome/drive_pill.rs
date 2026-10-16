use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Div, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px, relative,
};

use super::drives::DriveInfo;
use crate::file_list::row::inset_ring;
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Theme};

type OpenHandler = Rc<dyn Fn(&mut Window, &mut App)>;

const USAGE_HEIGHT: f32 = 2.;
const DIMMED: f32 = 0.55;
const LABEL_WIDTH: f32 = 96.;
const GLYPH: f32 = 14.;

#[derive(IntoElement)]
pub struct DrivePill {
    id: ElementId,
    drive: Option<DriveInfo>,
    is_busy: bool,
    is_open: bool,
    on_open: Option<OpenHandler>,
}

impl DrivePill {
    pub fn new(id: impl Into<ElementId>, drive: Option<DriveInfo>) -> Self {
        Self {
            id: id.into(),
            drive,
            is_busy: false,
            is_open: false,
            on_open: None,
        }
    }

    #[must_use]
    pub fn busy(mut self, is_busy: bool) -> Self {
        self.is_busy = is_busy;
        self
    }

    #[must_use]
    pub fn open(mut self, is_open: bool) -> Self {
        self.is_open = is_open;
        self
    }

    pub fn on_open(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(handler));
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

fn face(t: &Theme, drive: Option<&DriveInfo>, is_busy: bool) -> Div {
    let c = &t.colors;
    let glyph = match (is_busy, drive) {
        (true, _) => IconName::RefreshCw,
        (false, Some(drive)) => drive.icon.glyph(),
        (false, None) => IconName::HardDrive,
    };
    let letter: SharedString = match drive {
        Some(drive) if drive.letter.is_some() => format!("{}:", drive.name()).into(),
        Some(drive) => drive.name(),
        None => "Drives".into(),
    };
    let label = drive
        .filter(|drive| drive.letter.is_some())
        .and_then(|drive| drive.label.clone());
    div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .child(icon(glyph, px(GLYPH), c.text))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(letter))
        .when_some(label, |face, label| {
            face.child(
                div()
                    .max_w(px(LABEL_WIDTH))
                    .truncate()
                    .text_color(c.muted)
                    .child(label),
            )
        })
        .child(icon(IconName::ChevronDown, px(GLYPH), c.muted))
}

fn spoken_name(drive: &DriveInfo) -> SharedString {
    let name = match drive.letter {
        Some(_) => format!("{}:", drive.name()),
        None => drive.name().to_string(),
    };
    match &drive.label {
        Some(label) if drive.letter.is_some() => format!("Drive {name} {label}").into(),
        _ => format!("Drive {name}").into(),
    }
}

impl RenderOnce for DrivePill {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let was_open = window.use_keyed_state(self.id.clone(), cx, |_, _| false);
        let pressed = was_open.clone();
        let is_open = self.is_open;
        let t = tokens::current(cx);
        let c = &t.colors;
        let drive = self.drive.as_ref();
        let name = drive.map_or_else(|| "Drives".into(), spoken_name);
        let tip: Option<SharedString> = drive.map(|drive| drive.tooltip().into());
        let is_dimmed = self.is_busy || drive.is_some_and(|drive| !drive.connected);
        div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(name)
            .aria_expanded(self.is_open)
            .when_some(drive, |d, drive| {
                d.aria_value(status_value(drive, self.is_busy))
            })
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .h(t.spacing.tab_height)
            .px(t.spacing.md)
            .rounded(t.radii.md)
            .cursor_pointer()
            .text_color(c.text)
            .shadow(inset_ring(t.surfaces.card_edge))
            .hover(|style| style.bg(c.hover))
            .when(is_dimmed, |d| d.opacity(DIMMED))
            .when_some(tip, |d, tip| {
                d.tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            })
            .capture_any_mouse_down(move |_, _, cx| {
                pressed.update(cx, |state, _| *state = is_open);
            })
            .when_some(self.on_open, |d, handler| {
                d.on_click(move |_, window, cx| {
                    let was_open = *was_open.read(cx);
                    (!was_open).then(|| handler(window, cx));
                })
            })
            .child(face(t, drive, self.is_busy))
    }
}
