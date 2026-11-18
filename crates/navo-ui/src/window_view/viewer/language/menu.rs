//! Opens the language picker from the viewer and applies the row the user picks.
//! It follows the encoding picker: a menu under the header, an unsaved-changes guard, a reload.

use gpui_kit::{Bounds, FocusHandle, point};
use navo_core::shell_menu::ItemId;

use super::{choice_of, language_picker};
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
use crate::syntax_rules::rules;
use crate::tokens;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::syntax::{LanguageChoice, Pick, choose};

impl NavoWindow {
    /// Opens the language picker under the viewer header, or notes that the body has none.
    pub(in crate::window_view::viewer) fn open_language_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_language_menu_from(None, window, cx);
    }

    /// Opens the picker; Escape returns focus to `from`, else to what holds it now, else the body.
    pub(in crate::window_view::viewer) fn open_language_menu_from(
        &mut self,
        from: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let t = tokens::current(cx);
        let spacing = (t.spacing.xl, t.spacing.path_height);
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        let Some(body) = viewer.body.as_ref() else {
            return;
        };
        if body.language(cx).is_none() {
            viewer.say(navo_l10n::tr("viewer-language-not-text"), cx);
            cx.notify();
            return;
        }
        let sniff = viewer.sniff();
        let auto = choose(&Pick {
            name: sniff.name,
            head: sniff.head,
            choice: &LanguageChoice::Auto,
            len: Some(sniff.len),
        });
        let area = viewer.bounds.get();
        let restore = from
            .or_else(|| window.focused(cx))
            .unwrap_or_else(|| body.focus_handle(cx));
        let input = ContextMenuInput {
            menu: language_picker(&viewer.language, auto.name(), rules()),
            context: navo_l10n::tr("viewer-language-title").into(),
            anchor: point(area.right() - spacing.0, area.top() + spacing.1),
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(restore),
        };
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(input, window, cx);
            menu.hide_classic(cx);
            menu
        });
        let events = cx.subscribe_in(&menu, window, |this, _, event, window, cx| {
            this.language_menu_event(event, window, cx);
        });
        viewer.menu = Some((menu, events));
        viewer.is_view_menu = false;
        cx.notify();
    }

    /// Closes the picker and applies the chosen row; any other event only closes it.
    fn language_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = match event {
            ContextMenuEvent::OpenSubmenu(_) => return,
            ContextMenuEvent::Chosen(ItemId::Action(id)) => Some(*id),
            _ => None,
        };
        if let Some(viewer) = self.viewer.as_mut() {
            viewer.menu = None;
        }
        cx.notify();
        if let Some(id) = id {
            self.choose_language(choice_of(id), window, cx);
        }
    }

    /// Sets the viewer's language and reloads; unsaved edits ask for a second choice first.
    fn choose_language(
        &mut self,
        choice: LanguageChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = self.viewer.as_mut() else {
            return;
        };
        if viewer.language == choice {
            return;
        }
        if let Some(revision) = viewer.unsaved(cx)
            && viewer.relanguage.as_ref() != Some(&(revision, choice.clone()))
        {
            viewer.relanguage = Some((revision, choice));
            viewer.say(navo_l10n::tr("viewer-language-unsaved"), cx);
            cx.notify();
            return;
        }
        viewer.language = choice;
        self.reload_kind(window, cx);
    }
}
