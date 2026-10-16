mod actions;
mod prefetch;
mod provider;
mod run;
mod show;
mod targets;

use std::time::Duration;

use gpui_kit::Point;
use rusty_core::PaneId;
use rusty_core::shell_menu::{ItemId, MenuItem, ShellMenu, ferrite_actions};

pub use provider::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};

use super::FerriteWindow;
use crate::context_menu::{ContextMenu, ContextMenuEvent};
use crate::file_list::FileListAction;
use crate::prelude::*;
pub(super) use prefetch::Prefetch;
pub(super) use run::MenuRun;
use run::{MenuOpen, PendingView, Session, ShellCall, Step};

const POLL_EVERY: Duration = Duration::from_millis(16);
const WAIT_FOR_SHELL: Duration = Duration::from_millis(150);
const NO_SHELL: &str = "the Windows menu is not available";
const UNNAMED: &str = "Windows menu";

impl FerriteWindow {
    fn open_shell_menu(
        &mut self,
        (pane, anchor): (PaneId, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = self.menu_targets(pane) else {
            return;
        };
        let session = match self.shell_menus.clone() {
            Some(provider) => Session::Opening(
                self.take_prefetch(pane, &found.items)
                    .unwrap_or_else(|| provider.open(&found.items)),
            ),
            None => Session::Missing(NO_SHELL.to_owned()),
        };
        self.menu_runs += 1;
        let open = MenuOpen {
            id: self.menu_runs,
            show: PendingView {
                anchor,
                context: found.context.into(),
                restore: self.handle(pane, cx),
                until: cx.background_executor().now() + WAIT_FOR_SHELL,
            },
            pane,
            items: found.items,
            spot: targets::screen_spot(window, anchor),
            ferrite: ferrite_actions(),
            session,
        };
        self.menu_run = Some(MenuRun::new(open));
        self.poll_shell_menu(self.menu_runs, window, cx);
        self.keep_polling(window, cx);
    }

    pub(in crate::window_view) fn route_menu_action(
        &mut self,
        (pane, action): (PaneId, &FileListAction),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            FileListAction::ContextMenu(anchor) => {
                self.open_shell_menu((pane, *anchor), window, cx)
            }
            FileListAction::MenuPrefetch { adds_cursor } => {
                self.prefetch_shell_menu(pane, *adds_cursor);
            }
            FileListAction::MenuCancel => self.drop_prefetch(),
            _ => return false,
        }
        true
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
        self.keep_polling(window, cx);
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

    fn keep_polling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.menu_run.as_mut() else {
            return;
        };
        if !run.is_pending() || run.polling {
            return;
        }
        run.polling = true;
        let id = run.id;
        cx.spawn_in(window, async move |this, cx| {
            let mut more = true;
            while more {
                cx.background_executor().timer(POLL_EVERY).await;
                more = this
                    .update_in(cx, |view, window, cx| view.poll_shell_menu(id, window, cx))
                    .unwrap_or(false);
            }
        })
        .detach();
    }

    fn poll_shell_menu(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(run) = self.menu_run.as_mut().filter(|run| run.id == id) else {
            return false;
        };
        let mut step = run.poll();
        let pane = run.pane;
        let now = cx.background_executor().now();
        self.show_when_ready((id, &mut step, now), window, cx);
        let Some(run) = self.menu_run.as_mut().filter(|run| run.id == id) else {
            return false;
        };
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
