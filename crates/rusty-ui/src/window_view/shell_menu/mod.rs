mod actions;
mod background;
mod merge;
mod prefetch;
mod provider;
mod run;
mod show;
mod targets;

use std::time::{Duration, Instant};

use gpui_kit::{Focusable as _, Point};
use rusty_core::PaneId;
use rusty_core::shell_menu::{FerriteAction, ItemId, MenuItem, ShellMenu, ferrite_actions};

pub use background::{BackgroundLook, merged_background_menu};
pub use provider::{
    MenuTarget, Reply, ScreenSpot, ShellHealth, ShellMenuProvider, ShellMenuSession,
};

use super::FerriteWindow;
use crate::context_menu::{ContextMenu, ContextMenuEvent};
use crate::file_list::FileListAction;
use crate::prelude::*;
pub(super) use actions::put_paths_on_clipboard;
use merge::Own;
pub(super) use prefetch::Prefetches;
pub(super) use run::MenuRun;
use run::{MenuOpen, PendingView, RETRY, Session, ShellCall, Step};

const POLL_EVERY: Duration = Duration::from_millis(16);
const WAIT_FOR_SHELL: Duration = Duration::from_millis(150);
const NO_SHELL: &str = "the Windows menu is not available";
const UNNAMED: &str = "Windows menu";

impl FerriteWindow {
    fn open_shell_menu(
        &mut self,
        (pane, anchor, extended): (PaneId, Point<Pixels>, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = self.menu_targets(pane) else {
            return;
        };
        let target = MenuTarget::Items(found.items);
        let session = self.open_session(pane, &target, extended);
        let has_shell = self.shell_menus.is_some();
        let open = MenuOpen {
            id: self.menu_runs + 1,
            show: self.pending_view((pane, anchor), found.context.into(), cx),
            pane,
            target,
            extended,
            spot: targets::screen_spot(window, anchor),
            own: file_own(has_shell),
            bare: file_own(false),
            session,
            provider: self.shell_menus.clone(),
            opened: cx.background_executor().now(),
        };
        self.start_menu_run(open, window, cx);
    }

    fn open_background_menu(
        &mut self,
        (pane, anchor, extended): (PaneId, Point<Pixels>, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.browser.state().pane(pane).map(|pane| pane.active()) else {
            return;
        };
        let folder = tab.location().clone();
        let count = u64::try_from(self.browser.details(tab.id()).len()).unwrap_or(u64::MAX);
        let context = format!(
            "{} · {}",
            super::pane::title_of(tab),
            crate::format::count(count, "item", "items")
        );
        let look = BackgroundLook {
            sort: self.browser.sort_key(tab.id()),
            can_paste: self.has_files_to_paste(),
            has_properties: false,
        };
        let has_shell = self.shell_menus.is_some();
        let target = MenuTarget::Background(folder);
        let session = self.open_session(pane, &target, extended);
        let open = MenuOpen {
            id: self.menu_runs + 1,
            show: self.pending_view((pane, anchor), context.into(), cx),
            pane,
            target,
            extended,
            spot: targets::screen_spot(window, anchor),
            own: background::background_own(&look, has_shell),
            bare: background::background_own(&look, false),
            session,
            provider: self.shell_menus.clone(),
            opened: cx.background_executor().now(),
        };
        self.start_menu_run(open, window, cx);
    }

    fn open_session(&mut self, pane: PaneId, target: &MenuTarget, extended: bool) -> Session {
        let Some(provider) = self.shell_menus.clone() else {
            return Session::Missing(NO_SHELL.to_owned());
        };
        let prefetched = self.take_prefetch(pane, target, extended);
        Session::Opening(prefetched.unwrap_or_else(|| provider.open(target, extended)))
    }

    fn pending_view(
        &mut self,
        (pane, anchor): (PaneId, Point<Pixels>),
        context: SharedString,
        cx: &mut Context<Self>,
    ) -> PendingView {
        PendingView {
            anchor,
            context,
            restore: self.handle(pane, cx),
            until: cx.background_executor().now() + WAIT_FOR_SHELL,
            has_classic: self.shell_menus.is_some(),
        }
    }

    fn start_menu_run(&mut self, open: MenuOpen, window: &mut Window, cx: &mut Context<Self>) {
        crate::perf::begin(crate::perf::Latency::ContextMenu);
        self.menu_runs = open.id;
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
            FileListAction::ContextMenu { at, extended } => {
                self.open_shell_menu((pane, *at, *extended), window, cx);
            }
            FileListAction::BackgroundMenu { at, extended } => {
                self.open_background_menu((pane, *at, *extended), window, cx);
            }
            FileListAction::MenuPrefetch {
                adds_cursor,
                extended,
            } => self.prefetch_shell_menu(pane, *adds_cursor, *extended),
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
            ContextMenuEvent::Chosen(ItemId::Action(RETRY)) => self.retry_shell_menu(window, cx),
            ContextMenuEvent::Chosen(ItemId::Ferrite(action)) => {
                self.close_shell_menu();
                self.run_ferrite(*action, window, cx);
            }
            ContextMenuEvent::Chosen(ItemId::Action(name)) => {
                self.close_shell_menu();
                self.run_named(name, window, cx);
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

    fn retry_shell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let Some(run) = &mut self.menu_run else {
            return;
        };
        let view = run.view.as_ref().map(|(view, _)| view.clone());
        if let (Some(menu), Some(view)) = (run.retry(now), view) {
            view.update(cx, |view, cx| {
                view.set_menu(menu, cx);
                view.set_query("", window, cx);
                window.focus(&view.focus_handle(cx), cx);
            });
        }
    }

    fn close_shell_menu(&mut self) {
        crate::perf::cancel(crate::perf::Latency::ContextMenu);
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
        let now = cx.background_executor().now();
        let mut step = run.poll(now);
        let pane = run.pane;
        Self::fall_back(run, &mut step, now);
        self.show_when_ready((id, &mut step, now), window, cx);
        let Some(run) = self.menu_run.as_mut().filter(|run| run.id == id) else {
            return false;
        };
        let view = run.view.as_ref().map(|(view, _)| view.clone());
        let pending = run.is_pending();
        run.polling &= pending;
        let has_errors = !step.errors.is_empty();
        if has_errors || step.fallback {
            crate::perf::cancel(crate::perf::Latency::ContextMenu);
        }
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

    fn fall_back(run: &mut MenuRun, step: &mut Step, now: Instant) {
        if step.fallback
            && let Some(error) = run.fall_back()
        {
            step.errors.push(error);
            step.reopen = true;
        }
        if step.reopen {
            run.reopen(now);
        }
    }

    fn drop_finished_run(&mut self) {
        if self.menu_run.as_ref().is_some_and(MenuRun::is_done) {
            crate::perf::cancel(crate::perf::Latency::ContextMenu);
            self.menu_run = None;
        }
    }
}

fn ferrite_items(has_shell: bool) -> Vec<MenuItem> {
    let mut items = ferrite_actions();
    if !has_shell {
        items.retain(|item| {
            !matches!(
                item.id,
                ItemId::Ferrite(FerriteAction::Properties | FerriteAction::ClassicMenu)
            )
        });
    }
    items
}

fn file_own(has_shell: bool) -> Own {
    let items = ferrite_items(has_shell);
    if has_shell {
        Own {
            hidden: items,
            ..Own::default()
        }
    } else {
        Own {
            top: items,
            ..Own::default()
        }
    }
}

/// The menu a file selection shows on Windows: the shell list, with Navo's actions reached by typing.
#[must_use]
pub fn merged_file_menu(shell: &[MenuItem]) -> ShellMenu {
    merge::merge(shell, &file_own(true)).menu
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
