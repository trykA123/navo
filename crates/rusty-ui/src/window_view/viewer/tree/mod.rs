mod copy;
mod draw;
mod filter;
mod find;
mod jobs;
mod keys;
mod row;

use std::any::Any;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle, Task};
use rusty_structured::xml::Children;
use rusty_structured::{NodeId, Outline, ParseError, Step, Tree};

pub(in crate::window_view) use keys::FilterBack;
pub(super) use keys::bind_keys;

use super::background::Job;
use super::body::{BodyKeys, Host, MenuEntry, ViewerBody};
use super::search::view_find::ViewFind;
use crate::prelude::*;
use crate::primitives::ListScroll;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view::viewer) enum Format {
    Json,
    Jsonc,
    Yaml,
    Xml,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Jsonc => "JSONC",
            Self::Yaml => "YAML",
            Self::Xml => "XML",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Json => "Tree · JSON",
            Self::Jsonc => "Tree · JSONC",
            Self::Yaml => "Tree · YAML",
            Self::Xml => "Tree · XML",
        }
    }
}

pub(in crate::window_view::viewer) enum TreeLoad {
    Parsed(Box<Ready>),
    Failed(ParseError),
    Index,
}

pub(in crate::window_view::viewer) struct TreeFile {
    pub format: Format,
    pub len: u64,
    pub load: TreeLoad,
}

pub(in crate::window_view::viewer) struct Ready {
    tree: Arc<Tree>,
    outline: Outline,
    elements: Option<u64>,
}

impl Ready {
    pub(in crate::window_view::viewer) fn new(tree: Tree, elements: Option<u64>) -> Self {
        Self {
            outline: Outline::new(&tree),
            tree: Arc::new(tree),
            elements,
        }
    }
}

enum TreeState {
    Ready(Box<Ready>),
    Indexing(u64),
    Failed(ParseError),
    Stopped,
}

pub(in crate::window_view::viewer) struct TreeView {
    title: SharedString,
    format: Format,
    len: u64,
    host: Host,
    focus: FocusHandle,
    scroll: ListScroll,
    state: TreeState,
    note: Option<SharedString>,
    index_job: Option<Job>,
    loads: Vec<(NodeId, Job)>,
    page: usize,
    path: Option<SharedString>,
    filter: String,
    find: Option<ViewFind<NodeId>>,
    filter_wait: Option<Task<()>>,
    filter_job: Option<Job>,
    pending: Vec<(NodeId, Children)>,
    attach_wait: Option<Task<()>>,
}

impl TreeView {
    fn new(file: TreeFile, host: Host, cx: &mut Context<Self>) -> Self {
        let state = match file.load {
            TreeLoad::Parsed(ready) => TreeState::Ready(ready),
            TreeLoad::Failed(error) => TreeState::Failed(error),
            TreeLoad::Index => TreeState::Indexing(0),
        };
        let mut view = Self {
            title: host.title.clone(),
            format: file.format,
            len: file.len,
            host,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            state,
            note: None,
            index_job: None,
            loads: Vec::new(),
            page: 1,
            path: None,
            filter: String::new(),
            find: None,
            filter_wait: None,
            filter_job: None,
            pending: Vec::new(),
            attach_wait: None,
        };
        if matches!(view.state, TreeState::Indexing(_)) {
            view.index_job = Some(jobs::start_index(&view.host, view.len, cx));
        }
        view.sync_path();
        view
    }

    fn ready(&self) -> Option<&Ready> {
        match &self.state {
            TreeState::Ready(ready) => Some(ready),
            _ => None,
        }
    }

    fn ready_mut(&mut self) -> Option<&mut Ready> {
        match &mut self.state {
            TreeState::Ready(ready) => Some(ready),
            _ => None,
        }
    }

    fn sync_path(&mut self) {
        self.path = self.ready().and_then(|ready| {
            let node = ready.outline.selected()?;
            Some(ready.tree.path(node).into())
        });
    }

    fn follow(&mut self, step: Step, cx: &mut Context<Self>) {
        if let Step::Load(node, lazy) = step
            && !self.loads.iter().any(|(loading, _)| *loading == node)
        {
            let job = jobs::start_load(&self.host, node, lazy.span, cx);
            self.loads.push((node, job));
        }
        if let Some(ready) = self.ready() {
            self.scroll.scroll_to(ready.outline.cursor());
        }
        self.sync_path();
        self.note = None;
        cx.notify();
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    fn is_filtering(&self) -> bool {
        !self.filter.is_empty()
    }
}

pub(in crate::window_view::viewer) struct TreeBody(pub Entity<TreeView>);

impl TreeBody {
    pub(in crate::window_view::viewer) fn new(file: TreeFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| TreeView::new(file, host, cx)))
    }
}

impl ViewerBody for TreeBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, cx: &App) -> BodyKeys {
        BodyKeys::Normal(match &self.0.read(cx).state {
            TreeState::Ready(_) => vec![
                ("←/→", "Fold"),
                ("Alt+←/→", "Fold branch"),
                ("Ctrl+Shift+←/→", "Fold all"),
                ("Ctrl+1–9", "Depth"),
                ("Ctrl+C", "Copy value"),
                ("Ctrl+Shift+C", "Copy path"),
            ],
            TreeState::Indexing(_) => vec![("Esc", "Stop indexing")],
            TreeState::Failed(_) | TreeState::Stopped => Vec::new(),
        })
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |view, cx| view.find_next(backward, window, cx));
    }

    fn menu(&self, _: bool, cx: &mut App) -> Vec<MenuEntry> {
        match self.0.read(cx).state {
            TreeState::Ready(_) => keys::menu_entries(),
            _ => Vec::new(),
        }
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests;
