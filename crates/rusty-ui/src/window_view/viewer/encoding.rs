use gpui_kit::{Bounds, point};
use rusty_core::encoding::{DetectedBy, Encoding, TextEncoding, picker, picker_name};
use rusty_core::shell_menu::{Group, ItemId, MenuItem, ShellMenu};

use super::{ChooseEncoding, Viewer};
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens;
use crate::window_view::FerriteWindow;

const AUTO: &str = "auto";
const MENU_TITLE: &str = "Encoding";
const NOT_TEXT: &str = "Encodings apply to the text view. 1 switches to it.";
const UNSAVED: &str = "Unsaved changes. Ctrl+S saves; choose the encoding again to discard them.";

pub fn encoding_button(encoding: TextEncoding) -> impl IntoElement {
    let how = if encoding.by == DetectedBy::Chosen {
        "chosen"
    } else {
        "detected"
    };
    Button::new("viewer-encoding", encoding.label())
        .variant(ButtonVariant::Ghost)
        .tooltip(format!("Encoding, {how} (Ctrl+Shift+E)"))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ChooseEncoding), cx))
}

fn row(name: &'static str, text: &str, is_on: bool) -> MenuItem {
    MenuItem {
        checked: is_on,
        ..MenuItem::new(ItemId::Action(name), text, Group::Ferrite)
    }
}

#[must_use]
pub fn encoding_picker(current: &TextEncoding, detected: &TextEncoding) -> ShellMenu {
    let is_chosen = current.by == DetectedBy::Chosen;
    let auto = format!("Auto (detected: {})", detected.label());
    let mut items = vec![
        row(AUTO, &auto, !is_chosen),
        MenuItem::separator(Group::Ferrite),
    ];
    items.extend(picker().into_iter().map(|encoding| {
        let is_on = is_chosen && current.encoding == encoding;
        row(encoding.name(), &picker_name(encoding), is_on)
    }));
    ShellMenu { items }
}

impl Viewer {
    pub(super) fn note_detected(&mut self, cx: &App) {
        if self.encoding.is_none() {
            self.detected = self.body.as_ref().and_then(|body| body.encoding(cx));
        }
    }
}

impl FerriteWindow {
    pub(super) fn open_encoding_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let t = tokens::current(cx);
        let spacing = (t.spacing.xl, t.spacing.path_height);
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let Some(body) = viewer.body.as_ref() else {
            return;
        };
        let Some(current) = body.encoding(cx) else {
            viewer.say(NOT_TEXT, cx);
            cx.notify();
            return;
        };
        let detected = viewer.detected.unwrap_or(current);
        let area = viewer.bounds.get();
        let focus = body.focus_handle(cx);
        let input = ContextMenuInput {
            menu: encoding_picker(&current, &detected),
            context: MENU_TITLE.into(),
            anchor: point(area.right() - spacing.0, area.top() + spacing.1),
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(focus),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu.name_ferrite_group(MENU_TITLE, cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.encoding_menu_event(event, window, cx);
        });
        viewer.menu = Some((menu, events));
        viewer.is_view_menu = false;
        cx.notify();
    }

    fn encoding_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = match event {
            ContextMenuEvent::OpenSubmenu(_) => return,
            ContextMenuEvent::Chosen(ItemId::Action(name)) => Some(*name),
            _ => None,
        };
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.menu = None;
        }
        cx.notify();
        match name {
            Some(AUTO) => self.choose_encoding(None, window, cx),
            Some(name) => {
                let encoding = Encoding::for_label(name.as_bytes());
                self.choose_encoding(encoding, window, cx);
            }
            None => {}
        }
    }

    fn choose_encoding(
        &mut self,
        encoding: Option<&'static Encoding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        if viewer.encoding == encoding {
            return;
        }
        if let Some(revision) = viewer.unsaved(cx)
            && viewer.reencoding != Some(revision)
        {
            viewer.reencoding = Some(revision);
            viewer.say(UNSAVED, cx);
            cx.notify();
            return;
        }
        viewer.encoding = encoding;
        self.reload_kind(window, cx);
    }
}
