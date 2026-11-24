//! The encoding picker (Ctrl+Shift+E): re-reads the file in another encoding and, while the F3
//! editor is editing, chooses the encoding and line ending the next save converts to (E7c).

use std::sync::LazyLock;

use gpui_kit::{Bounds, point};
use navo_core::encoding::{DetectedBy, Encoding, TextEncoding, picker, picker_name};
use navo_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};

use super::{ChooseEncoding, LineEnding, SaveAs, Viewer};
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant};
use crate::tokens;
use crate::window_view::NavoWindow;

const AUTO: &str = "auto";
const MENU_TITLE: &str = "Encoding";
const NOT_TEXT: &str = "Encodings apply to the text view. 1 switches to it.";
const UNSAVED: &str = "Unsaved changes. Ctrl+S saves; choose the encoding again to discard them.";
const SAVE_PREFIX: &str = "save-as:";
const SAVE_MENU: &str = "save-as";
const ENDINGS_MENU: &str = "line-endings";
const ENDINGS: [(&str, &str, LineEnding); 3] = [
    (
        "eol-loaded",
        "viewer-line-endings-loaded",
        LineEnding::AsLoaded,
    ),
    ("eol-lf", "viewer-line-endings-lf", LineEnding::Lf),
    ("eol-crlf", "viewer-line-endings-crlf", LineEnding::Crlf),
];

/// One menu id per picker encoding for its "Save as" row; built once, as menu ids are `'static`.
static SAVE_IDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    picker()
        .into_iter()
        .map(|encoding| &*format!("{SAVE_PREFIX}{}", encoding.name()).leak())
        .collect()
});

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
        ..MenuItem::new(ItemId::Action(name), text, Group::Navo)
    }
}

/// A submenu row `id` titled `text` holding `children`.
fn submenu(id: &'static str, text: &str, children: Vec<MenuItem>) -> MenuItem {
    MenuItem {
        kind: ItemKind::Submenu(Submenu::Filled(children)),
        ..MenuItem::new(ItemId::Action(id), text, Group::Navo)
    }
}

/// The "Save as" and "Line endings" submenus for a file loaded as `loaded`, with `plan` checked.
fn save_rows(loaded: TextEncoding, plan: SaveAs) -> Vec<MenuItem> {
    let encodings = picker()
        .into_iter()
        .zip(SAVE_IDS.iter())
        .map(|(encoding, id)| {
            let target = SaveAs {
                encoding: Some(encoding),
                ..plan
            };
            let name = target.encoding_for(loaded).label();
            let text = navo_l10n::t!("viewer-save-as-encoding", name = name);
            row(
                id,
                &text,
                plan.encoding.unwrap_or(loaded.encoding) == encoding,
            )
        })
        .collect();
    let endings = ENDINGS
        .iter()
        .map(|(id, key, ending)| row(id, &navo_l10n::tr(key), plan.line_ending == *ending))
        .collect();
    vec![
        MenuItem::separator(Group::Navo),
        submenu(SAVE_MENU, &navo_l10n::tr("viewer-save-as"), encodings),
        submenu(ENDINGS_MENU, &navo_l10n::tr("viewer-line-endings"), endings),
    ]
}

/// The conversion menu id `name` asks for on top of `plan`, or None when it is not a save row.
fn save_choice(name: &str, plan: SaveAs) -> Option<SaveAs> {
    if let Some(label) = name.strip_prefix(SAVE_PREFIX) {
        let encoding = Encoding::for_label(label.as_bytes())?;
        return Some(SaveAs {
            encoding: Some(encoding),
            ..plan
        });
    }
    let (_, _, line_ending) = ENDINGS.iter().find(|(id, ..)| *id == name)?;
    Some(SaveAs {
        line_ending: *line_ending,
        ..plan
    })
}

#[must_use]
pub fn encoding_picker(current: &TextEncoding, detected: &TextEncoding) -> ShellMenu {
    let is_chosen = current.by == DetectedBy::Chosen;
    let auto = format!("Auto (detected: {})", detected.label());
    let mut items = vec![
        row(AUTO, &auto, !is_chosen),
        MenuItem::separator(Group::Navo),
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

    /// After a save that converted the file, reads it in the new encoding from then on, so a
    /// reload never decodes the new bytes with the old encoding.
    pub(super) fn adopt_saved_encoding(&mut self, cx: &App) {
        let saved = self.body.as_ref().and_then(|body| body.encoding(cx));
        if let Some(saved) = saved.filter(|saved| saved.by == DetectedBy::Chosen) {
            self.encoding = Some(saved.encoding);
        }
    }
}

impl NavoWindow {
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
        let mut menu = encoding_picker(&current, &detected);
        if let Some(plan) = body.save_as(cx) {
            menu.items.extend(save_rows(current, plan));
        }
        let input = ContextMenuInput {
            menu,
            context: MENU_TITLE.into(),
            anchor: point(area.right() - spacing.0, area.top() + spacing.1),
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(focus),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
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
        let body = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
        let plan = body.and_then(|body| body.save_as(cx));
        match name {
            Some(AUTO) => self.choose_encoding(None, window, cx),
            Some(name) => {
                if let Some(next) = plan.and_then(|plan| save_choice(name, plan)) {
                    body.into_iter().for_each(|body| body.set_save_as(next, cx));
                } else if let Some(encoding) = Encoding::for_label(name.as_bytes()) {
                    self.choose_encoding(Some(encoding), window, cx);
                }
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
            && viewer.reencoding != Some((revision, encoding))
        {
            viewer.reencoding = Some((revision, encoding));
            viewer.say(UNSAVED, cx);
            cx.notify();
            return;
        }
        viewer.encoding = encoding;
        self.reload_kind(window, cx);
    }
}
