use gpui_kit::{Focusable, Subscription};

use crate::prelude::*;
use crate::shortcuts::{ShortcutsEvent, ShortcutsPage};
use crate::window_view::{FerriteWindow, KeymapNews};

const NO_FILE: &str = "There is no keymap file in this session.";

pub(super) struct ShortcutsPanel {
    page: Entity<ShortcutsPage>,
    _events: Subscription,
}

impl FerriteWindow {
    pub(in crate::window_view) fn open_key_map(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(window, cx);
        if self.settings.is_open {
            self.show_shortcuts(window, cx);
            cx.notify();
        }
    }

    pub(super) fn open_guide_from_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_settings(window, cx);
        self.open_user_guide(window, cx);
    }

    pub(super) fn show_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let page = self.shortcuts_entity(window, cx);
        self.settings.is_shortcuts = true;
        self.settings.edit = None;
        self.settings.notice = None;
        let focus = page.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    fn shortcuts_entity(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ShortcutsPage> {
        if let Some(panel) = &self.settings.shortcuts {
            return panel.page.clone();
        }
        let save = self.keymap.save.clone();
        let problem = self.keymap.problem.clone();
        let page = cx.new(|cx| {
            let mut page = ShortcutsPage::new(save, window, cx);
            page.set_problem(problem, cx);
            page
        });
        let events = cx.subscribe_in(&page, window, |view, _, event, window, cx| {
            view.shortcuts_event(event, window, cx);
        });
        self.settings.shortcuts = Some(ShortcutsPanel {
            page: page.clone(),
            _events: events,
        });
        page
    }

    fn shortcuts_event(
        &mut self,
        event: &ShortcutsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ShortcutsEvent::Saved(saved) => {
                let news = KeymapNews::Applied {
                    keymap: saved.keymap.clone(),
                    warnings: saved.warnings.clone(),
                };
                self.take_keymap(news, cx);
            }
            ShortcutsEvent::OpenFile => match self.keymap.file.clone() {
                Some(file) => self.view_location(file, window, cx),
                None => self.settings.notice = Some(NO_FILE.into()),
            },
        }
        cx.notify();
    }

    pub(in crate::window_view) fn tell_shortcuts(
        &self,
        problem: Option<SharedString>,
        cx: &mut App,
    ) {
        if let Some(panel) = &self.settings.shortcuts {
            panel
                .page
                .update(cx, |page, cx| page.set_problem(problem, cx));
        }
    }

    pub(super) fn pause_shortcuts(&self, cx: &mut App) {
        if let Some(panel) = &self.settings.shortcuts {
            panel.page.update(cx, |page, cx| page.pause(cx));
        }
    }

    pub(super) fn shown_shortcuts(&self) -> Option<&Entity<ShortcutsPage>> {
        let panel = self.settings.shortcuts.as_ref()?;
        self.settings.is_shortcuts.then_some(&panel.page)
    }
}

#[cfg(test)]
impl FerriteWindow {
    pub(in crate::window_view) fn is_showing_shortcuts(&self) -> bool {
        self.shown_shortcuts().is_some()
    }

    pub(in crate::window_view) fn shortcuts_page(&self) -> Option<Entity<ShortcutsPage>> {
        self.settings
            .shortcuts
            .as_ref()
            .map(|panel| panel.page.clone())
    }
}
