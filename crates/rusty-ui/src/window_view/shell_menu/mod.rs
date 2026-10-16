mod actions;
mod provider;
mod run;
mod targets;

use std::time::Duration;

use gpui_kit::{Bounds, Point};
use rusty_core::PaneId;
use rusty_core::shell_menu::{ItemId, MenuItem, ShellMenu, ferrite_actions};

pub use provider::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};

use super::FerriteWindow;
use crate::context_menu::{ContextMenu, ContextMenuEvent, ContextMenuInput};
use crate::prelude::*;
pub(super) use run::MenuRun;
use run::{MenuOpen, Session, ShellCall, Step};

const POLL_EVERY: Duration = Duration::from_millis(16);
const NO_SHELL: &str = "the Windows menu is not available";
const UNNAMED: &str = "Windows menu";

impl FerriteWindow {
    pub(super) fn open_shell_menu(
        &mut self,
        (pane, anchor): (PaneId, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.browser.state().pane(pane).map(|p| p.active().clone()) else {
            return;
        };
        let found = targets::targets(&tab, &self.browser.details(tab.id()));
        if found.items.is_empty() {
            return;
        }
        let ferrite = ferrite_actions();
        let mut items = ferrite.clone();
        let session = match &self.shell_menus {
            Some(provider) => {
                items.push(targets::loading_item());
                Session::Opening(provider.open(&found.items))
            }
            None => Session::Missing(NO_SHELL.to_owned()),
        };
        let input = ContextMenuInput {
            menu: ShellMenu { items },
            context: found.context.into(),
            anchor,
            bounds: Bounds::new(Point::default(), window.viewport_size()),
            restore: Some(self.handle(pane, cx)),
        };
        let view = cx.new(|cx| ContextMenu::new(input, window, cx));
        let events = cx.subscribe_in(&view, window, |this, _, event, window, cx| {
            this.shell_menu_event(event, window, cx);
        });
        self.menu_runs += 1;
        let open = MenuOpen {
            id: self.menu_runs,
            pane,
            items: found.items,
            spot: targets::screen_spot(window, anchor),
            ferrite,
            session,
        };
        self.menu_run = Some(MenuRun::new(open, (view, events)));
        self.poll_shell_menu(self.menu_runs, cx);
        self.keep_polling(cx);
        cx.notify();
    }

    pub(super) fn shell_menu_view(&self) -> Option<Entity<ContextMenu>> {
        let run = self.menu_run.as_ref()?;
        run.view.as_ref().map(|(view, _)| view.clone())
    }

    fn shell_menu_event(
        &mut self,
        event: &ContextMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ContextMenuEvent::OpenSubmenu(path) => {
                if let Some(run) = &mut self.menu_run {
                    run.fill(path);
                }
            }
            ContextMenuEvent::Dismissed => self.close_shell_menu(),
            ContextMenuEvent::ShowClassic => {
                self.close_shell_menu();
                self.start_shell_call(ShellCall::Classic);
            }
            ContextMenuEvent::Chosen(ItemId::Ferrite(action)) => {
                self.close_shell_menu();
                self.run_ferrite(*action, window, cx);
            }
            ContextMenuEvent::Chosen(id) => {
                let label = self
                    .shell_menu_view()
                    .and_then(|view| label_of(view.read(cx).menu(), id))
                    .unwrap_or_else(|| UNNAMED.to_owned());
                self.close_shell_menu();
                let id = id.clone();
                self.start_shell_call(ShellCall::Invoke { id, label });
            }
        }
        self.keep_polling(cx);
        self.drop_finished_run();
        cx.notify();
    }

    fn close_shell_menu(&mut self) {
        if let Some(run) = &mut self.menu_run {
            run.view = None;
        }
    }

    fn start_shell_call(&mut self, call: ShellCall) {
        let Some(run) = &mut self.menu_run else {
            return;
        };
        let pane = run.pane;
        if let Some(error) = run.start(call) {
            self.report_pane_error(pane, error.into());
        }
    }

    fn keep_polling(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.menu_run.as_mut() else {
            return;
        };
        if !run.is_pending() || run.polling {
            return;
        }
        run.polling = true;
        let id = run.id;
        cx.spawn(async move |this, cx| {
            let mut more = true;
            while more {
                cx.background_executor().timer(POLL_EVERY).await;
                more = this
                    .update(cx, |view, cx| view.poll_shell_menu(id, cx))
                    .unwrap_or(false);
            }
        })
        .detach();
    }

    fn poll_shell_menu(&mut self, id: u64, cx: &mut Context<Self>) -> bool {
        let Some(run) = self.menu_run.as_mut().filter(|run| run.id == id) else {
            return false;
        };
        let step = run.poll();
        let pane = run.pane;
        let view = run.view.as_ref().map(|(view, _)| view.clone());
        let pending = run.is_pending();
        run.polling &= pending;
        let has_errors = !step.errors.is_empty();
        Self::apply_step(view, &step, cx);
        for error in step.errors {
            self.report_pane_error(pane, error.into());
        }
        self.drop_finished_run();
        if has_errors {
            cx.notify();
        }
        pending
    }

    fn apply_step(view: Option<Entity<ContextMenu>>, step: &Step, cx: &mut Context<Self>) {
        let Some(view) = view.filter(|_| step.menu.is_some() || !step.submenus.is_empty()) else {
            return;
        };
        view.update(cx, |menu, cx| {
            if let Some(items) = &step.menu {
                menu.set_menu(items.clone(), cx);
            }
            for (path, submenu) in &step.submenus {
                menu.set_submenu(path, submenu.clone(), cx);
            }
        });
    }

    fn drop_finished_run(&mut self) {
        if self.menu_run.as_ref().is_some_and(MenuRun::is_done) {
            self.menu_run = None;
        }
    }
}

fn label_of(menu: &ShellMenu, id: &ItemId) -> Option<String> {
    fn find<'a>(items: &'a [MenuItem], id: &ItemId) -> Option<&'a MenuItem> {
        items.iter().find_map(|item| {
            if item.id == *id && !item.is_submenu() {
                return Some(item);
            }
            item.children().and_then(|children| find(children, id))
        })
    }
    find(&menu.items, id).map(|item| item.text.clone())
}
