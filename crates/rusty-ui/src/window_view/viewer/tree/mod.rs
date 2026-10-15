mod draw;
mod jobs;
mod keys;
mod row;

use std::any::Any;

use gpui_kit::{AnyView, FocusHandle};
use rusty_structured::{NodeId, Outline, ParseError, Step, Tree};

pub(super) use keys::bind_keys;

use super::background::Job;
use super::body::{BodyKeys, Host, ViewerBody};
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
    Parsed(Tree),
    Failed(ParseError),
    Index,
}

pub(in crate::window_view::viewer) struct TreeFile {
    pub format: Format,
    pub len: u64,
    pub load: TreeLoad,
}

struct Ready {
    tree: Tree,
    outline: Outline,
    elements: Option<u64>,
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
}

impl TreeView {
    fn new(file: TreeFile, host: Host, cx: &mut Context<Self>) -> Self {
        let state = match file.load {
            TreeLoad::Parsed(tree) => TreeState::Ready(Box::new(Ready {
                outline: Outline::new(&tree),
                tree,
                elements: None,
            })),
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
        };
        if matches!(view.state, TreeState::Indexing(_)) {
            view.index_job = Some(jobs::start_index(&view.host, view.len, cx));
        }
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

    fn selected_path(&self) -> Option<SharedString> {
        let ready = self.ready()?;
        let node = ready.outline.selected()?;
        Some(ready.tree.path(node).into())
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
        self.note = None;
        cx.notify();
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }

    fn is_filtering(&self) -> bool {
        self.ready()
            .is_some_and(|ready| !ready.outline.filter().is_empty())
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
                ("*", "Expand all"),
                ("-", "Collapse all"),
                ("Ctrl+Shift+C", "Copy path"),
            ],
            TreeState::Indexing(_) => vec![("Esc", "Stop indexing")],
            TreeState::Failed(_) | TreeState::Stopped => Vec::new(),
        })
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
