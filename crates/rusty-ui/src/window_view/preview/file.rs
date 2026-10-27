use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::Task;
use rusty_fs::{JobSlot, OwnerGuard, Priority, volume_of};

use super::follow::Target;
use super::{Preview, Shown};
use crate::prelude::*;
use crate::window_view::FerriteWindow;
use crate::window_view::viewer::{
    Attach, FileHooks, Host, Kind, Opened, Picked, Request, SaveError, Turn, ViewerBody,
    sniff_and_load,
};

const READ_ONLY: &str = "The preview is read-only.";

struct Loading {
    _owner: OwnerGuard,
    _task: Task<()>,
    _alive: Arc<()>,
}

pub(in crate::window_view) struct FilePreview {
    pub target: Target,
    pub kind: Option<Kind>,
    pub body: Option<Box<dyn ViewerBody>>,
    pub message: Option<SharedString>,
    loading: Option<Loading>,
}

impl FilePreview {
    pub(super) fn is_loading(&self) -> bool {
        self.loading.is_some()
    }

    pub(super) fn stop(&mut self) -> bool {
        self.loading.take().is_some()
    }
}

pub(super) fn read_only(hooks: &FileHooks) -> FileHooks {
    FileHooks {
        write: Arc::new(|_, _| Err(SaveError::Failed(READ_ONLY.to_owned()))),
        save_memory: Arc::new(|_, _, _, _| {}),
        remember: Arc::new(|_| {}),
        ..hooks.clone()
    }
}

impl Preview {
    fn file_mut(&mut self) -> Option<&mut FilePreview> {
        match &mut self.shown {
            Some(Shown::File(file)) => Some(file),
            _ => None,
        }
    }
}

impl FerriteWindow {
    pub(super) fn start_file(
        &self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FilePreview {
        let request = Request {
            hooks: read_only(&self.files),
            location: target.location.clone(),
            name: target.name(),
            encoding: None,
        };
        let kinds = self.kinds.clone();
        let owner = self.preview.reads.owner();
        let slot = JobSlot {
            volume: volume_of(&target.location),
            priority: Priority::Visible,
            owner: owner.id(),
        };
        let (sender, receiver) = futures::channel::oneshot::channel();
        let alive = Arc::new(());
        let weak = Arc::downgrade(&alive);
        self.preview.reads.submit_then(
            slot,
            move |cancel| {
                let turn = Turn {
                    alive: &weak,
                    cancel: cancel.as_atomic_bool(),
                };
                sniff_and_load(&request, &kinds, turn)
            },
            move |opened| {
                let _ = sender.send(opened);
            },
        );
        let task = cx.spawn_in(window, async move |this, cx| {
            let opened = receiver
                .await
                .unwrap_or_else(|_| Err(rusty_l10n::tr("preview-load-failed")));
            let _ = this.update_in(cx, |view, window, cx| {
                view.preview_opened(opened, window, cx)
            });
        });
        FilePreview {
            target,
            kind: None,
            body: None,
            message: None,
            loading: Some(Loading {
                _owner: owner,
                _task: task,
                _alive: alive,
            }),
        }
    }

    fn preview_host(&self, target: &Target, window: &Window, cx: &mut Context<Self>) -> Host {
        Host {
            title: target.name().into(),
            location: target.location.clone(),
            hooks: read_only(&self.files),
            on_saved: Rc::new(|_| {}),
            open: Self::open_later(cx.weak_entity(), window.window_handle()),
            carried: None,
            is_preview: true,
        }
    }

    fn preview_opened(
        &mut self,
        opened: Result<Opened, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.preview.file_mut().map(|file| file.target.clone()) else {
            return;
        };
        let host = self.preview_host(&target, window, cx);
        let Some(file) = self.preview.file_mut() else {
            return;
        };
        file.loading = None;
        let Picked { kind, loaded, note } = match opened {
            Ok(opened) => opened.picked,
            Err(message) => {
                file.message = Some(message.into());
                cx.notify();
                return;
            }
        };
        let attach = |loaded| {
            let attach = Attach {
                loaded,
                host,
                edit: false,
            };
            kind.attach(attach, window, cx)
        };
        match loaded.and_then(attach) {
            Ok(body) => {
                if let Some(note) = note {
                    body.set_note(note.into(), cx);
                }
                file.body = Some(body);
            }
            Err(message) => file.message = Some(message.into()),
        }
        file.kind = Some(kind);
        if self.preview.browse.is_focused(window) {
            let target = self.preview.browse_target(cx);
            window.focus(&target, cx);
        }
        cx.notify();
    }
}
