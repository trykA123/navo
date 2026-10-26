use std::mem;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{Entity, FocusHandle, Pixels, Point, SharedString, Subscription};
use rusty_core::PaneId;
use rusty_core::shell_menu::{
    FerriteAction, Group, ItemId, MenuItem, ShellMenu, Submenu, ferrite_actions,
};
use rusty_l10n::tr;

use super::merge::{self, Own, PROPERTIES_VERB};
use super::provider::{MenuTarget, Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};
use crate::context_menu::ContextMenu;

pub(super) const RETRY: &str = "shell_menu.retry";
const SLOW_NOTE: &str = "shell_menu.slow";
const LOADING_NOTE: &str = "shell_menu.loading";
const SLOW_AFTER: Duration = Duration::from_secs(2);
const WINDOWS_MENU: &str = "Windows menu";

pub(super) enum Session {
    Opening(Reply<Box<dyn ShellMenuSession>>),
    Ready(Box<dyn ShellMenuSession>),
    Missing(String),
}

pub(super) enum ShellCall {
    Invoke { id: ItemId, label: String },
    Classic,
    Properties,
}

impl ShellCall {
    fn label(&self) -> &str {
        match self {
            Self::Invoke { label, .. } => label,
            Self::Classic => "Classic menu",
            Self::Properties => "Properties",
        }
    }
}

#[derive(Default)]
pub(super) struct Step {
    pub menu: Option<ShellMenu>,
    pub submenus: Vec<(Vec<usize>, Submenu)>,
    pub errors: Vec<String>,
    pub fallback: bool,
    pub reopen: bool,
}

#[derive(Clone)]
pub(super) struct PendingView {
    pub anchor: Point<Pixels>,
    pub context: SharedString,
    pub restore: FocusHandle,
    pub until: Instant,
    pub has_classic: bool,
}

pub(super) struct MenuOpen {
    pub id: u64,
    pub show: PendingView,
    pub pane: PaneId,
    pub target: MenuTarget,
    pub extended: bool,
    pub spot: ScreenSpot,
    pub own: Own,
    pub bare: Own,
    pub session: Session,
    pub provider: Option<Rc<dyn ShellMenuProvider>>,
    pub opened: Instant,
}

struct Call {
    label: String,
    reply: Reply<()>,
    is_fallback: bool,
}

pub(in crate::window_view) struct MenuRun {
    pub(super) id: u64,
    pub(super) pane: PaneId,
    pub(super) target: MenuTarget,
    pub(super) view: Option<(Entity<ContextMenu>, Subscription)>,
    pub(super) show: Option<PendingView>,
    pub(super) polling: bool,
    again: PendingView,
    extended: bool,
    spot: ScreenSpot,
    own: Own,
    bare: Own,
    layout: Vec<Option<usize>>,
    session: Session,
    provider: Option<Rc<dyn ShellMenuProvider>>,
    open_error: Option<String>,
    opened_at: Instant,
    is_slow: bool,
    fills: Vec<(Vec<usize>, Reply<Submenu>)>,
    calls: Vec<Call>,
    waiting: Vec<ShellCall>,
}

impl MenuRun {
    pub(super) fn new(open: MenuOpen) -> Self {
        Self {
            id: open.id,
            pane: open.pane,
            target: open.target,
            view: None,
            opened_at: open.opened,
            again: open.show.clone(),
            show: Some(open.show),
            polling: false,
            extended: open.extended,
            spot: open.spot,
            own: open.own,
            bare: open.bare,
            layout: Vec::new(),
            session: open.session,
            provider: open.provider,
            open_error: None,
            is_slow: false,
            fills: Vec::new(),
            calls: Vec::new(),
            waiting: Vec::new(),
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        self.show.is_some()
            || matches!(self.session, Session::Opening(_))
            || !self.fills.is_empty()
            || !self.calls.is_empty()
    }

    pub(super) fn is_done(&self) -> bool {
        self.view.is_none()
            && self.show.is_none()
            && self.calls.is_empty()
            && self.waiting.is_empty()
    }

    pub(super) fn start(&mut self, call: ShellCall) -> Option<String> {
        let reply = match &self.session {
            Session::Opening(_) if self.is_slow && matches!(call, ShellCall::Classic) => {
                let error = self.classic_for(false);
                self.session = Session::Missing(super::NO_SHELL.to_owned());
                return error;
            }
            Session::Opening(_) => {
                self.waiting.push(call);
                return None;
            }
            Session::Missing(message) => return Some(format!("{}: {message}", call.label())),
            Session::Ready(session) => match &call {
                ShellCall::Invoke { id, .. } => session.invoke(id, self.spot),
                ShellCall::Classic => session.classic(self.spot),
                ShellCall::Properties => session.invoke(&properties(), self.spot),
            },
        };
        self.calls.push(Call {
            label: call.label().to_owned(),
            reply,
            is_fallback: false,
        });
        None
    }

    fn classic_for(&mut self, is_fallback: bool) -> Option<String> {
        let label = ShellCall::Classic.label().to_owned();
        let Some(provider) = &self.provider else {
            return Some(format!("{label}: {}", super::NO_SHELL));
        };
        let reply = provider.classic_for(&self.target, self.extended, self.spot);
        self.calls.push(Call {
            label,
            reply,
            is_fallback,
        });
        None
    }

    /// Hands the click to the native menu; a failure there brings back Navo's own list.
    pub(super) fn fall_back(&mut self) -> Option<String> {
        self.view = None;
        self.show = None;
        self.waiting.clear();
        let reply = match &self.session {
            Session::Ready(session) => session.classic(self.spot),
            Session::Opening(_) | Session::Missing(_) => return self.classic_for(true),
        };
        self.calls.push(Call {
            label: ShellCall::Classic.label().to_owned(),
            reply,
            is_fallback: true,
        });
        None
    }

    pub(super) fn reopen(&mut self, now: Instant) {
        self.own = self.bare.clone();
        self.show = Some(PendingView {
            until: now,
            has_classic: false,
            ..self.again.clone()
        });
    }

    pub(super) fn retry(&mut self, now: Instant) -> Option<ShellMenu> {
        let provider = self.provider.clone()?;
        self.session = Session::Opening(provider.open(&self.target, self.extended));
        self.opened_at = now;
        self.is_slow = false;
        self.fills.clear();
        Some(self.first_menu())
    }

    pub(super) fn is_opening(&self) -> bool {
        matches!(self.session, Session::Opening(_))
    }

    pub(super) fn first_menu(&mut self) -> ShellMenu {
        let shell = if self.is_slow {
            slow_items()
        } else if self.is_opening() {
            vec![note(LOADING_NOTE, &tr("shell-menu-loading"))]
        } else {
            Vec::new()
        };
        let merged = merge::merge(&shell, &self.own);
        self.layout = merged.shell_at;
        merged.menu
    }

    pub(super) fn fill(&mut self, path: &[usize]) {
        let is_asked = self.fills.iter().any(|(asked, _)| asked == path);
        if let (false, Session::Ready(session)) = (is_asked, &self.session)
            && let Some(shell_path) = merge::shell_path(&self.layout, path)
        {
            self.fills.push((path.to_vec(), session.fill(&shell_path)));
        }
    }

    pub(super) fn poll(&mut self, now: Instant) -> Step {
        let mut step = Step::default();
        self.poll_session(&mut step);
        if !self.is_slow && self.is_opening() && now >= self.opened_at + SLOW_AFTER {
            self.is_slow = true;
            step.menu = Some(self.first_menu());
        }
        self.poll_fills(&mut step);
        self.poll_calls(&mut step);
        step
    }

    fn poll_session(&mut self, step: &mut Step) {
        let Session::Opening(reply) = &self.session else {
            return;
        };
        let Some(answer) = reply() else {
            return;
        };
        match answer {
            Ok(session) if session.menu().has_named_item() => {
                let mut merged = merge::merge(&session.menu().items, &self.own);
                let lazy = merge::mark_loading(&mut merged.menu.items, &[]);
                self.layout = merged.shell_at;
                step.menu = Some(merged.menu);
                self.session = Session::Ready(session);
                lazy.iter().for_each(|path| self.fill(path));
            }
            Ok(session) => {
                self.session = Session::Ready(session);
                step.fallback = true;
            }
            Err(message) => {
                self.session = Session::Missing(message.clone());
                self.open_error = Some(message);
                step.fallback = true;
            }
        }
        if step.fallback {
            return;
        }
        for call in mem::take(&mut self.waiting) {
            step.errors.extend(self.start(call));
        }
    }

    fn poll_fills(&mut self, step: &mut Step) {
        let mut lazy = Vec::new();
        for (path, reply) in mem::take(&mut self.fills) {
            match reply() {
                None | Some(Ok(Submenu::Loading)) => self.fills.push((path, reply)),
                Some(Ok(Submenu::Filled(mut items))) => {
                    lazy.extend(merge::mark_loading(&mut items, &path));
                    step.submenus.push((path, Submenu::Filled(items)));
                }
                Some(Ok(other)) => step.submenus.push((path, other)),
                Some(Err(_)) => step.submenus.push((path, Submenu::Failed)),
            }
        }
        lazy.iter().for_each(|path| self.fill(path));
    }

    fn poll_calls(&mut self, step: &mut Step) {
        let open_error = self.open_error.clone();
        self.calls.retain(|call| match (call.reply)() {
            None => true,
            Some(Ok(())) => false,
            Some(Err(message)) if call.is_fallback => {
                step.reopen = true;
                step.errors.push(match &open_error {
                    Some(open) => format!("{WINDOWS_MENU}: {open}"),
                    None => format!("{}: {message}", call.label),
                });
                false
            }
            Some(Err(message)) => {
                step.errors.push(format!("{}: {message}", call.label));
                false
            }
        });
    }
}

fn note(id: &'static str, text: &str) -> MenuItem {
    MenuItem {
        enabled: false,
        ..MenuItem::new(ItemId::Action(id), text, Group::Other)
    }
}

fn slow_items() -> Vec<MenuItem> {
    let classic = ItemId::Ferrite(FerriteAction::ClassicMenu);
    let classic = ferrite_actions()
        .into_iter()
        .filter(|item| item.id == classic);
    let retry = MenuItem::new(
        ItemId::Action(RETRY),
        &tr("context-menu-retry"),
        Group::Other,
    );
    let mut items = vec![note(SLOW_NOTE, &tr("shell-menu-slow"))];
    items.extend(classic);
    items.push(retry);
    items
}

fn properties() -> ItemId {
    ItemId::Shell {
        command: 0,
        verb: Some(PROPERTIES_VERB.to_owned()),
    }
}
