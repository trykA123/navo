use std::time::Duration;

use gpui_kit::Task;
use rusty_core::Location;

use super::modes::Shown;
use super::{FileHooks, Head, Kind, Kinds, Loaded, Sniff, Viewer};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const SLOW_OPEN: Duration = Duration::from_secs(2);

pub(super) struct Opening {
    pub is_slow: bool,
    _task: Task<()>,
    _timer: Task<()>,
}

pub(super) struct Opened {
    head: Head,
    kinds: Vec<Kind>,
    kind: Kind,
    loaded: Result<Loaded, String>,
}

pub(super) struct Request {
    pub hooks: FileHooks,
    pub location: Location,
    pub name: String,
}

fn sniff_and_load(request: &Request, kinds: &Kinds) -> Result<Opened, String> {
    let head = (request.hooks.head)(&request.location)?;
    let sniff = Sniff {
        head: &head.bytes,
        name: &request.name,
        len: head.len,
    };
    let chosen = kinds.choose(&sniff);
    let kind = chosen
        .first()
        .cloned()
        .ok_or_else(|| "Nothing here can show this file.".to_owned())?;
    let loaded = kind.load(&request.hooks, &request.location, &sniff);
    Ok(Opened {
        head,
        kinds: chosen,
        kind,
        loaded,
    })
}

pub(super) fn start<T: Send + 'static>(
    window: &mut Window,
    cx: &mut Context<FerriteWindow>,
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut FerriteWindow, T, &mut Window, &mut Context<FerriteWindow>) + 'static,
) -> Opening {
    let (sender, receiver) = futures::channel::oneshot::channel();
    cx.background_spawn(async move {
        let _ = sender.send(work());
    })
    .detach();
    let task = cx.spawn_in(window, async move |this, cx| {
        let Ok(result) = receiver.await else {
            return;
        };
        let _ = this.update_in(cx, |view, window, cx| done(view, result, window, cx));
    });
    let timer = cx.spawn(async move |this, cx| {
        cx.background_executor().timer(SLOW_OPEN).await;
        let _ = this.update(cx, |view, cx| view.open_is_slow(cx));
    });
    Opening {
        is_slow: false,
        _task: task,
        _timer: timer,
    }
}

impl FerriteWindow {
    pub(super) fn request(&self, viewer: &Viewer) -> Request {
        Request {
            hooks: self.files.clone(),
            location: viewer.location.clone(),
            name: viewer.title.to_string(),
        }
    }

    pub(super) fn open_file(
        &self,
        viewer: &Viewer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Opening {
        let request = self.request(viewer);
        let kinds = self.kinds.clone();
        start(
            window,
            cx,
            move || sniff_and_load(&request, &kinds),
            Self::file_opened,
        )
    }

    fn open_is_slow(&mut self, cx: &mut Context<Self>) {
        if let Some(opening) = self
            .viewer
            .as_mut()
            .and_then(|viewer| viewer.opening.as_mut())
        {
            opening.is_slow = true;
            cx.notify();
        }
    }

    fn file_opened(
        &mut self,
        opened: Result<Opened, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        match opened {
            Ok(opened) => {
                viewer.head = opened.head;
                viewer.kinds = opened.kinds;
                let shown = Shown {
                    kind: opened.kind,
                    loaded: opened.loaded,
                    edit: false,
                };
                self.show_loaded(shown, window, cx);
            }
            Err(message) => {
                viewer.opening = None;
                viewer.message = Some(message.into());
                cx.notify();
            }
        }
    }
}
