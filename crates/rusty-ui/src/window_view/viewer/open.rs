use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use gpui_kit::Task;
use rusty_core::Location;

use super::cancel::Cancel;
use super::kinds::{IMAGE_ID, PDF_ID};
use super::modes::Shown;
use super::{Declined, FileHooks, Head, Kind, Kinds, Load, Loaded, Slot, Sniff, Viewer};
use crate::prelude::*;
use crate::window_view::FerriteWindow;

const SLOW_OPEN: Duration = Duration::from_secs(2);

const CLOSED: &str = "The viewer closed before the file opened.";

static IMAGE_LOADING: Mutex<()> = Mutex::new(());
static PDF_OPENING: Mutex<()> = Mutex::new(());

pub(super) fn load_while_alive(
    alive: &Weak<()>,
    kind: &Kind,
    load: impl FnOnce() -> Result<Loaded, String>,
) -> Result<Loaded, String> {
    let turn = match kind.id() {
        IMAGE_ID => Some(&IMAGE_LOADING),
        PDF_ID => Some(&PDF_OPENING),
        _ => None,
    };
    let _turn = turn.map(|turn| turn.lock().unwrap_or_else(PoisonError::into_inner));
    if alive.strong_count() == 0 {
        return Err(CLOSED.to_owned());
    }
    load()
}

pub(in crate::window_view) struct Picked {
    pub kind: Kind,
    pub loaded: Result<Loaded, String>,
    pub note: Option<&'static str>,
}

pub(super) fn load_or_text(
    kinds: &[Kind],
    kind: Kind,
    load: impl Fn(&Kind) -> Result<Loaded, String>,
) -> Picked {
    let loaded = load(&kind);
    let declined = match &loaded {
        Ok(loaded) => loaded.downcast_ref::<Declined>().map(|declined| declined.0),
        Err(_) => None,
    };
    let Some(note) = declined else {
        return Picked {
            kind,
            loaded,
            note: None,
        };
    };
    match kinds.iter().find(|kind| kind.slot() == Slot::Text) {
        Some(text) => Picked {
            kind: text.clone(),
            loaded: load(text),
            note: Some(note),
        },
        None => Picked {
            kind,
            loaded: Err(note.to_owned()),
            note: None,
        },
    }
}

pub(super) struct Opening {
    pub is_slow: bool,
    alive: Option<Arc<()>>,
    _task: Task<()>,
    _timer: Task<()>,
    cancel: Cancel,
}

impl Opening {
    pub(super) fn supersede(&mut self) {
        self.alive = None;
        self.cancel.cancel();
    }

    fn is_superseded(&self) -> bool {
        self.alive.is_none()
    }
}

#[cfg(test)]
impl Opening {
    pub(super) fn cancel_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.cancel.flag()
    }
}

pub(in crate::window_view) struct Opened {
    pub head: Head,
    pub kinds: Vec<Kind>,
    pub picked: Picked,
}

pub(in crate::window_view) struct Request {
    pub hooks: FileHooks,
    pub location: Location,
    pub name: String,
}

#[derive(Clone, Copy)]
pub(in crate::window_view) struct Turn<'a> {
    pub alive: &'a Weak<()>,
    pub cancel: &'a AtomicBool,
}

impl Request {
    pub(super) fn loader<'a>(
        &'a self,
        sniff: &'a Sniff<'_>,
        turn: Turn<'a>,
    ) -> impl Fn(&Kind) -> Result<Loaded, String> + 'a {
        move |kind| {
            load_while_alive(turn.alive, kind, || {
                kind.load(Load {
                    hooks: &self.hooks,
                    location: &self.location,
                    sniff,
                    cancel: turn.cancel,
                })
            })
        }
    }
}

pub(in crate::window_view) fn sniff_and_load(
    request: &Request,
    kinds: &Kinds,
    turn: Turn<'_>,
) -> Result<Opened, String> {
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
    let mut picked = load_or_text(&chosen, kind, request.loader(&sniff, turn));
    picked.note = picked.note.or_else(|| kinds.declined(&sniff));
    Ok(Opened {
        head,
        kinds: chosen,
        picked,
    })
}

pub(super) fn start<T: Send + 'static>(
    window: &mut Window,
    cx: &mut Context<FerriteWindow>,
    work: impl FnOnce(Turn<'_>) -> T + Send + 'static,
    done: impl FnOnce(&mut FerriteWindow, T, &mut Window, &mut Context<FerriteWindow>) + 'static,
) -> Opening {
    let cancel = Cancel::new();
    let flag = cancel.flag();
    let (sender, receiver) = futures::channel::oneshot::channel();
    let alive = Arc::new(());
    let weak = Arc::downgrade(&alive);
    cx.background_spawn(async move {
        let turn = Turn {
            alive: &weak,
            cancel: &flag,
        };
        let _ = sender.send(work(turn));
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
        alive: Some(alive),
        _task: task,
        _timer: timer,
        cancel,
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
            move |turn| sniff_and_load(&request, &kinds, turn),
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

    pub(super) fn restart_if_superseded(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(viewer) = self
            .viewer
            .as_ref()
            .filter(|viewer| viewer.opening.as_ref().is_some_and(Opening::is_superseded))
        else {
            return false;
        };
        let opening = self.open_file(viewer, window, cx);
        if let Some(viewer) = &mut self.viewer {
            viewer.opening = Some(opening);
        }
        cx.notify();
        true
    }

    fn file_opened(
        &mut self,
        opened: Result<Opened, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_if_superseded(window, cx) {
            return;
        }
        let Some(viewer) = &mut self.viewer else {
            return;
        };
        match opened {
            Ok(opened) => {
                viewer.head = opened.head;
                viewer.kinds = opened.kinds;
                let shown = Shown {
                    picked: opened.picked,
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
