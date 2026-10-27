mod browse;
mod contents;
mod contents_view;
mod file;
mod folder;
mod follow;
mod frame;
mod header;
mod inbox;
mod marks;
mod nest;
mod scan;
mod sheet;
mod spot;
mod stats;
mod summary;
mod view;

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{FocusHandle, Task};
use rusty_fs::{IoQueue, JobKind, PROGRESS_EVERY, Pools};

use crate::browser::Waker;

use super::FerriteWindow;
use crate::prelude::*;
use crate::side_sheet::SheetState;
pub(in crate::window_view) use browse::bind_keys;
use file::FilePreview;
use folder::FolderPreview;
use follow::{Key, Target};
pub use summary::folder_summary;

const FOLLOW_AFTER: Duration = Duration::from_millis(150);

pub(in crate::window_view) enum Shown {
    Empty,
    File(Box<FilePreview>),
    Folder(Box<FolderPreview>),
}

impl Shown {
    fn key(&self) -> Option<Key> {
        match self {
            Self::Empty => None,
            Self::File(file) => Some(file.target.key()),
            Self::Folder(folder) => Some(folder.target.key()),
        }
    }

    fn stop(&mut self) -> bool {
        match self {
            Self::Empty => false,
            Self::File(file) => file.stop(),
            Self::Folder(folder) => folder.stop(),
        }
    }
}

pub(in crate::window_view) struct Preview {
    sheet: Entity<SheetState>,
    browse: FocusHandle,
    is_open: bool,
    seen: Option<Key>,
    is_stale: bool,
    wait: Option<Task<()>>,
    forget: Option<Task<()>>,
    shown: Option<Shown>,
    trail: Vec<nest::Crumb>,
    view: view::PreviewView,
    reads: Arc<IoQueue>,
    scans: Arc<IoQueue>,
    inbox: inbox::Inbox,
    every: Duration,
    ids: u64,
    loads: u64,
}

#[cfg(not(test))]
fn pools() -> &'static Pools {
    Pools::shared()
}

#[cfg(test)]
fn pools() -> &'static Pools {
    Box::leak(Box::new(Pools::new()))
}

impl Preview {
    pub(super) fn new(wake: Waker, cx: &mut App) -> Self {
        Self {
            sheet: cx.new(|_| SheetState::new()),
            browse: cx.focus_handle(),
            is_open: false,
            seen: None,
            is_stale: false,
            wait: None,
            forget: None,
            shown: None,
            trail: Vec::new(),
            view: view::PreviewView::default(),
            reads: Arc::clone(pools().queue(JobKind::Preview)),
            scans: Arc::clone(pools().queue(JobKind::Size)),
            inbox: inbox::Inbox::new(wake),
            every: PROGRESS_EVERY,
            ids: 0,
            loads: 0,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.is_open
    }

    fn stop_loading(&mut self) {
        if let Some(shown) = &mut self.shown {
            self.is_stale |= shown.stop();
        }
    }
}

impl FerriteWindow {
    pub(in crate::window_view) fn open_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preview.is_open = true;
        self.preview.forget = None;
        self.preview.sheet.update(cx, |sheet, cx| sheet.open(cx));
        self.preview.shown = None;
        self.preview.seen = self.cursor_target().map(|target| target.key());
        self.show_target(window, cx);
    }

    pub(in crate::window_view) fn close_preview(&mut self, cx: &mut Context<Self>) {
        self.preview.is_open = false;
        self.preview.sheet.update(cx, |sheet, cx| sheet.close(cx));
        self.preview.wait = None;
        self.preview.seen = None;
        self.preview.trail.clear();
        self.preview.clear_marks();
        self.preview.view.reset();
        self.preview.stop_loading();
        let fade = crate::tokens::current(cx).motion.fade;
        self.preview.forget = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(fade).await;
            let _ = this.update(cx, |view, cx| view.forget_preview(cx));
        }));
    }

    fn forget_preview(&mut self, cx: &mut Context<Self>) {
        self.preview.forget = None;
        if !self.preview.is_open {
            self.preview.shown = None;
            cx.notify();
        }
    }

    pub(in crate::window_view) fn follow_cursor(
        &mut self,
        _: Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.preview.is_open || self.viewer.is_some() {
            return;
        }
        let key = self.cursor_target().map(|target| target.key());
        if key == self.preview.seen {
            return;
        }
        let is_same_place = matches!(
            (&key, &self.preview.seen),
            (Some(next), Some(seen)) if next.same_place(seen)
        );
        self.preview.seen = key;
        if is_same_place && !self.preview.trail.is_empty() {
            return;
        }
        if !is_same_place {
            self.preview.stop_loading();
        }
        self.preview.wait = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(FOLLOW_AFTER).await;
            let _ = this.update_in(cx, |view, window, cx| view.show_target(window, cx));
        }));
    }

    fn show_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview.wait = None;
        let target = self.cursor_target();
        let key = target.as_ref().map(Target::key);
        let is_same = self.preview.shown.as_ref().map(Shown::key) == Some(key);
        if is_same && !self.preview.is_stale {
            return;
        }
        self.preview.is_stale = false;
        self.preview.trail.clear();
        let was_browsing = self.preview.is_browsing(window, cx);
        let same_folder = self.preview.folder_mut().filter(|folder| {
            target
                .as_ref()
                .is_some_and(|target| target.location == folder.target.location)
        });
        let (carried, scan, marks) = same_folder.map_or((None, None, None), |folder| {
            let marks = std::mem::take(&mut folder.marks);
            (folder.carry(), folder.scan.take_session(), Some(marks))
        });
        let shown = match target {
            None => Shown::Empty,
            Some(target) if target.is_folder() => {
                let mut folder = self.start_folder(target, scan);
                if let Some(carry) = carried {
                    folder.adopt(carry);
                }
                folder.marks = marks.unwrap_or_default();
                Shown::Folder(Box::new(folder))
            }
            Some(target) => Shown::File(Box::new(self.start_file(target, window, cx))),
        };
        if !matches!(shown, Shown::Empty) {
            self.preview.loads += 1;
        }
        self.preview.shown = Some(shown);
        self.apply_preview_parts();
        if was_browsing {
            let target = self.preview.browse_target(cx);
            window.focus(&target, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
impl Preview {
    pub(super) fn loads(&self) -> u64 {
        self.loads
    }

    pub(super) fn set_every(&mut self, every: Duration) {
        self.every = every;
    }

    pub(super) fn shown(&self) -> Option<&Shown> {
        self.shown.as_ref()
    }
}
