use std::rc::Rc;

use gpui_kit::{AnchoredPositionMode, FocusHandle, Subscription, anchored, deferred};
use navo_core::Location;
use navo_l10n::t;

use super::NavoWindow;
use crate::commands;
use crate::confirm_dialog::{ConfirmDialog, ConfirmProps};
use crate::file_list::OpenGitMenu;
use crate::git::menu::{self, Choice, Model, Source};
use crate::git::menu_view::{self, Live, Look};
use crate::git::{GitAction, GitMenu, GitStarted, Previewed, RepoBadge};
use crate::prelude::*;
use crate::primitives::popup_menu::{self, ChooseHandler, Keys, PopupMenu, wire_keys};
use crate::status_bar::StatusNotice;
use crate::tokens;

const GAP: f32 = 4.;
const MARGIN: f32 = 8.;

/// What the hooks and the badge gave; the model is rebuilt from it when previews arrive.
pub(super) struct Parts {
    menu: GitMenu,
    badge: RepoBadge,
    tool: Option<String>,
    can_diff: bool,
}

impl Parts {
    pub(super) fn is_pending(&self) -> bool {
        self.menu.items.iter().any(|item| item.pending)
    }

    pub(super) fn model(&self, previews: Option<&Previewed>) -> Model {
        menu::build(&Source {
            menu: &self.menu,
            badge: &self.badge,
            tool: self.tool.as_deref(),
            can_diff: self.can_diff,
            previews,
        })
    }
}

pub(super) struct Open {
    pub(super) repo: Location,
    listing: Location,
    at: Point<Pixels>,
    pub(super) parts: Parts,
    pub(super) model: Model,
}

/// The last reply the open menu accepted; Git commands outside the menu read it.
pub(super) struct Cached {
    pub(super) repo: Location,
    pub(super) replies: Previewed,
}

/// A menu row to run, with the confirmation the user saw and its id, if the row had one.
struct Run {
    repo: Location,
    action: GitAction,
    label: String,
    shown: Option<String>,
    confirm_id: Option<String>,
}

struct Ask {
    run: Run,
    text: String,
}

pub(super) struct GitMenuState {
    popup: Entity<PopupMenu>,
    pub(super) open: Option<Open>,
    ask: Option<Ask>,
    asks: u64,
    pub(super) requests: u64,
    pub(super) cache: Option<Cached>,
    _watch: Subscription,
}

impl GitMenuState {
    pub(super) fn popup(&self) -> &Entity<PopupMenu> {
        &self.popup
    }

    pub(super) fn is_open(&self, cx: &App) -> bool {
        self.open.is_some() && self.popup.read(cx).is_open
    }

    pub(super) fn new(cx: &mut Context<NavoWindow>) -> Self {
        let popup = cx.new(|cx| PopupMenu::new(cx));
        let watch = cx.observe(&popup, |_, _, cx| cx.notify());
        Self {
            popup,
            open: None,
            ask: None,
            asks: 0,
            requests: 0,
            cache: None,
            _watch: watch,
        }
    }
}

fn plain(label: &str) -> String {
    label.trim_end_matches('…').trim_end().to_owned()
}

fn no_reply() -> super::lane::Reply {
    Box::new(|_, _, _| {})
}

impl NavoWindow {
    fn git_menu_target(&self, (row, at): (u64, Point<Pixels>)) -> Option<Open> {
        let hooks = self.git_badges.hooks()?;
        let state = self.browser.state();
        let pane = state.focused_pane();
        let tab = state.pane(pane)?.active();
        let name = tab.listing()?.get(row)?.name.clone();
        let badge = self.git_badges.badges(pane)?.by_name.get(&name)?.clone();
        let repo = tab.entry_location(&name).ok()?;
        let tool = hooks.tools().into_iter().find(|tool| tool.selected);
        let parts = Parts {
            menu: hooks.menu(&repo),
            badge,
            tool: tool.map(|tool| tool.label),
            can_diff: hooks.can_diff(&repo),
        };
        let cached = self.git_menu.cache.as_ref().filter(|c| c.repo == repo);
        let model = parts.model(cached.map(|c| &c.replies));
        let listing = tab.location().clone();
        (!model.rows.is_empty()).then_some(Open {
            repo,
            listing,
            at,
            parts,
            model,
        })
    }

    pub(super) fn open_git_menu(
        &mut self,
        target: (u64, Point<Pixels>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_menu.cache = None;
        let Some(open) = self.git_menu_target(target) else {
            return;
        };
        self.close_popups(window, cx);
        self.git_menu.open = Some(open);
        popup_menu::open(&self.git_menu.popup, window, cx);
        self.request_git_previews(cx);
        cx.notify();
    }

    /// Runs a Git command on the cursor row when the menu offers it, enabled; else nothing.
    pub(super) fn run_git_command(
        &mut self,
        (row, action): (u64, GitAction),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.git_menu_target((row, Point::default())) else {
            return;
        };
        let wanted = |row: &menu::Row| row.enabled && row.choice == Choice::Run(action);
        let Some(index) = open.model.rows.iter().position(wanted) else {
            return;
        };
        self.git_menu.open = Some(open);
        self.git_chosen(index, window, cx);
    }

    /// The Git commands the menu offers, enabled, for the cursor row.
    pub(super) fn git_offered(&self) -> Vec<&'static str> {
        let state = self.browser.state();
        let tab = state.pane(state.focused_pane()).map(|pane| pane.active());
        let row = tab.map_or(0, |tab| tab.view().cursor);
        let Some(open) = self.git_menu_target((row, Point::default())) else {
            return Vec::new();
        };
        let runs = open.model.rows.iter().filter(|row| row.enabled);
        let runs = runs.filter_map(|row| match row.choice {
            Choice::Run(action) => commands::git::name_of(action),
            Choice::Recheck => None,
        });
        std::iter::once(commands::git::MENU).chain(runs).collect()
    }

    /// The palette entries: clipboard ones need clipboard hooks, Git ones need an offering row.
    pub(super) fn palette_entries(&self) -> Vec<commands::CommandEntry> {
        let offered = self.git_offered();
        let shown = |entry: &commands::CommandEntry| {
            !commands::git::is_git(entry.spec.name) || offered.contains(&entry.spec.name)
        };
        let mut entries = self.command_entries();
        entries.retain(shown);
        entries
    }

    pub(super) fn close_git_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        popup_menu::close(&self.git_menu.popup, window, cx);
    }

    pub(super) fn git_menu_holds(&self, focused: &FocusHandle, cx: &App) -> bool {
        self.git_menu.popup.read(cx).holds(focused)
    }

    fn git_key_hint(&self, window: &Window) -> Option<String> {
        let pane = self.browser.state().focused_pane();
        let focus = self.focus.get(&pane)?;
        let binding = window.highest_precedence_binding_for_action_in(&OpenGitMenu, focus)?;
        binding
            .keystrokes()
            .first()
            .map(|keystroke| keystroke.inner().unparse())
    }

    fn git_chooser(cx: &mut Context<Self>) -> ChooseHandler {
        let this = cx.weak_entity();
        Rc::new(move |index, window, cx| {
            let _ = this.update(cx, |view, cx| view.git_chosen(index, window, cx));
        })
    }

    pub(super) fn git_menu_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let popup = &self.git_menu.popup;
        let open = self.git_menu.open.as_ref()?;
        let selected = self.git_menu.is_open(cx).then(|| popup.read(cx).selected)?;
        let theme = tokens::current(cx).clone();
        let hint = self.git_key_hint(window);
        let on_choose = Self::git_chooser(cx);
        let live = Live {
            state: popup.clone(),
            on_choose: on_choose.clone(),
        };
        let look = Look {
            theme: &theme,
            selected: Some(selected),
            hint: hint.as_deref(),
        };
        let panel = menu_view::live(&open.model, &look, &live);
        let keys = Keys {
            state: popup.clone(),
            count: open.model.rows.len(),
            on_choose,
        };
        let placed = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(open.at)
            .offset(point(px(0.), px(GAP)))
            .snap_to_window_with_margin(px(MARGIN))
            .child(wire_keys(panel, keys, cx));
        Some(deferred(placed).with_priority(2).into_any_element())
    }

    fn git_chosen(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.git_menu.open else {
            return;
        };
        let Some(row) = open.model.rows.get(index).cloned() else {
            return;
        };
        let (repo, listing) = (open.repo.clone(), open.listing.clone());
        if !row.enabled {
            self.git_notice(row.note, false);
            return cx.notify();
        }
        let Choice::Run(action) = row.choice else {
            self.git_rescan(listing);
            return cx.notify();
        };
        if action == GitAction::ChooseTool {
            self.open_settings(window, cx);
            return cx.notify();
        }
        let run = Run {
            repo,
            action,
            label: plain(&row.label),
            shown: row.confirm.clone(),
            confirm_id: row.confirm_id,
        };
        match row.confirm {
            Some(text) => {
                self.git_menu.asks += 1;
                self.git_menu.ask = Some(Ask { run, text });
            }
            None => self.git_run(run, cx),
        }
        cx.notify();
    }

    fn git_rescan(&mut self, listing: Location) {
        self.git_menu.cache = None;
        let Some(hooks) = self.git_badges.hooks() else {
            return;
        };
        self.lane.push(Box::new(move || {
            hooks.rescan(&listing);
            no_reply()
        }));
    }

    fn git_notice(&mut self, text: String, is_error: bool) {
        self.show_notice(StatusNotice {
            text: text.into(),
            is_error,
        });
    }

    fn git_run(&mut self, run: Run, cx: &mut Context<Self>) {
        let Some(hooks) = self.git_badges.hooks() else {
            return;
        };
        let Run {
            repo,
            action,
            label,
            shown,
            confirm_id,
        } = run;
        let work = cx.background_spawn(async move {
            hooks.run_confirmed(&repo, action, shown.as_deref(), confirm_id.as_deref())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |view, cx| view.git_ran(&label, result, cx));
        })
        .detach();
    }

    fn git_ran(&mut self, label: &str, result: Result<GitStarted, String>, cx: &mut Context<Self>) {
        self.git_menu.cache = None;
        match result {
            Ok(started) => {
                let text = started
                    .result
                    .unwrap_or_else(|| t!("git-menu-started", action = label, tool = started.tool));
                self.git_notice(text, false);
            }
            Err(error) => self.git_notice(error, true),
        }
        cx.notify();
    }

    pub(super) fn git_ask_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ask = self.git_menu.ask.as_ref()?;
        let props = ConfirmProps {
            title: ask.run.label.clone().into(),
            lines: Vec::new(),
            more: 0,
            summary: ask.text.clone().into(),
            confirm_label: ask.run.label.clone().into(),
            danger: false,
            typed: None,
            mode: None,
            choice: None,
        };
        let (confirm, cancel) = (cx.weak_entity(), cx.weak_entity());
        let dialog = ConfirmDialog::new(("git-ask", self.git_menu.asks), props)
            .on_confirm(move |window, cx| {
                let _ = confirm.update(cx, |view, cx| view.git_answered(true, window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = cancel.update(cx, |view, cx| view.git_answered(false, window, cx));
            });
        Some(
            div()
                .id("git-ask-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(tokens::current(cx).surfaces.scrim)
                .occlude()
                .child(dialog)
                .into_any_element(),
        )
    }

    fn git_answered(&mut self, is_confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let asked = self.git_menu.ask.take();
        self.focus_core_pane(window, cx);
        if let (true, Some(ask)) = (is_confirmed, asked) {
            self.git_run(ask.run, cx);
        }
        cx.notify();
    }
}
