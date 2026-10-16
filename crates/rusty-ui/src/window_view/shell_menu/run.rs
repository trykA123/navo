use std::mem;

use std::time::Instant;

use gpui_kit::{Entity, FocusHandle, Pixels, Point, SharedString, Subscription};
use rusty_core::shell_menu::{Group, ItemId, ItemKind, MenuItem, ShellMenu, Submenu};
use rusty_core::{Location, PaneId};

use super::provider::{Reply, ScreenSpot, ShellMenuSession};
use crate::context_menu::ContextMenu;

const PROPERTIES_VERB: &str = "properties";
const LOADING: &str = "Loading Windows items…";

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
}

pub(super) struct PendingView {
    pub anchor: Point<Pixels>,
    pub context: SharedString,
    pub restore: FocusHandle,
    pub until: Instant,
}

pub(super) struct MenuOpen {
    pub id: u64,
    pub show: PendingView,
    pub pane: PaneId,
    pub items: Vec<Location>,
    pub spot: ScreenSpot,
    pub ferrite: Vec<MenuItem>,
    pub session: Session,
}

pub(in crate::window_view) struct MenuRun {
    pub(super) id: u64,
    pub(super) pane: PaneId,
    pub(super) items: Vec<Location>,
    pub(super) view: Option<(Entity<ContextMenu>, Subscription)>,
    pub(super) show: Option<PendingView>,
    pub(super) polling: bool,
    spot: ScreenSpot,
    ferrite: Vec<MenuItem>,
    session: Session,
    fills: Vec<(Vec<usize>, Reply<Submenu>)>,
    calls: Vec<(String, Reply<()>)>,
    waiting: Vec<ShellCall>,
}

impl MenuRun {
    pub(super) fn new(open: MenuOpen) -> Self {
        Self {
            id: open.id,
            pane: open.pane,
            items: open.items,
            view: None,
            show: Some(open.show),
            polling: false,
            spot: open.spot,
            ferrite: open.ferrite,
            session: open.session,
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
        self.calls.push((call.label().to_owned(), reply));
        None
    }

    pub(super) fn is_opening(&self) -> bool {
        matches!(self.session, Session::Opening(_))
    }

    pub(super) fn first_menu(&self) -> ShellMenu {
        let mut items = self.ferrite.clone();
        if self.is_opening() {
            items.push(loading_item());
        }
        ShellMenu { items }
    }

    pub(super) fn fill(&mut self, path: &[usize]) {
        let is_asked = self.fills.iter().any(|(asked, _)| asked == path);
        if let (false, Session::Ready(session)) = (is_asked, &self.session)
            && let Some(shell_path) = shell_path(path, self.ferrite.len())
        {
            self.fills.push((path.to_vec(), session.fill(&shell_path)));
        }
    }

    pub(super) fn poll(&mut self) -> Step {
        let mut step = Step::default();
        self.poll_session(&mut step);
        self.poll_fills(&mut step);
        self.calls.retain(|(label, reply)| match reply() {
            None => true,
            Some(Ok(())) => false,
            Some(Err(message)) => {
                step.errors.push(format!("{label}: {message}"));
                false
            }
        });
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
            Ok(session) => {
                let mut items = self.ferrite.clone();
                items.extend(session.menu().items.iter().cloned());
                let offset = self.ferrite.len();
                let lazy = mark_loading(&mut items[offset..], &[], offset);
                step.menu = Some(ShellMenu { items });
                self.session = Session::Ready(session);
                for path in lazy {
                    self.fill(&path);
                }
            }
            Err(message) => {
                step.menu = Some(ShellMenu {
                    items: self.ferrite.clone(),
                });
                step.errors.push(format!("Windows menu: {message}"));
                self.session = Session::Missing(message);
            }
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
                    lazy.extend(mark_loading(&mut items, &path, 0));
                    step.submenus.push((path, Submenu::Filled(items)));
                }
                Some(Ok(other)) => step.submenus.push((path, other)),
                Some(Err(_)) => step.submenus.push((path, Submenu::Failed)),
            }
        }
        for path in lazy {
            self.fill(&path);
        }
    }
}

fn loading_item() -> MenuItem {
    let id = ItemId::Shell {
        command: 0,
        verb: None,
    };
    MenuItem {
        enabled: false,
        ..MenuItem::new(id, LOADING, Group::Other)
    }
}

fn properties() -> ItemId {
    ItemId::Shell {
        command: 0,
        verb: Some(PROPERTIES_VERB.to_owned()),
    }
}

fn shell_path(path: &[usize], offset: usize) -> Option<Vec<usize>> {
    let (first, rest) = path.split_first()?;
    let first = first.checked_sub(offset)?;
    Some([&[first], rest].concat())
}

fn mark_loading(items: &mut [MenuItem], parent: &[usize], offset: usize) -> Vec<Vec<usize>> {
    let mut lazy = Vec::new();
    for (index, item) in items.iter_mut().enumerate() {
        let path = [parent, &[index + offset]].concat();
        match &mut item.kind {
            ItemKind::Submenu(submenu @ Submenu::Lazy) => {
                *submenu = Submenu::Loading;
                lazy.push(path);
            }
            ItemKind::Submenu(Submenu::Filled(children)) => {
                lazy.extend(mark_loading(children, &path, 0));
            }
            _ => {}
        }
    }
    lazy
}
