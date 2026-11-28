use gpui_kit::{Focusable, Subscription};

use crate::prelude::*;
use crate::shortcuts::{FailedSave, SaveKeymap, ShortcutsEvent, ShortcutsPage};
use crate::window_view::{KeymapNews, NavoWindow};

const NO_FILE: &str = "There is no keymap file in this session.";

pub(super) struct ShortcutsPanel {
    page: Entity<ShortcutsPage>,
    _events: Subscription,
}

impl ShortcutsPanel {
    pub(super) fn take_failed_save(&self, cx: &mut App) -> Option<FailedSave> {
        self.page.update(cx, |page, _| page.take_failed_save())
    }
}

fn new_shortcuts_page(
    save: SaveKeymap,
    problem: Option<SharedString>,
    failed: Option<FailedSave>,
    window: &mut Window,
    cx: &mut Context<ShortcutsPage>,
) -> ShortcutsPage {
    let mut page = ShortcutsPage::new(save, window, cx);
    page.set_problem(problem, cx);
    if let Some(failed) = failed {
        page.restore_failed_save(failed, cx);
    }
    page
}

impl NavoWindow {
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
        self.settings.is_shortcuts = true;
        self.settings.edit = None;
        self.settings.notice = None;
        if super::super::settings_window::on() && !self.settings.host.is_open() {
            return;
        }
        let page = self.shortcuts_entity(window, cx);
        let focus = page.read(cx).focus_handle(cx);
        self.settings
            .host
            .with_window_or(window, cx, |target, cx| target.focus(&focus, cx));
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
        let failed = self.settings.failed_shortcut.take();
        let page = self.settings.host.with_window_or(window, cx, |target, cx| {
            let save = save.clone();
            let problem = problem.clone();
            let failed = failed.clone();
            cx.new(|cx| new_shortcuts_page(save, problem, failed, target, cx))
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
impl NavoWindow {
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
