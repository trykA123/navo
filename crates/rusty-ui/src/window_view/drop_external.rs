use gpui_kit::{
    DispatchPhase, DragMoveEvent, ExternalDragPayload, ExternalPaths, FileDragPaths, FileDropEvent,
    MouseUpEvent,
};
use rusty_core::{DropKeys, DropOp, Location};

use super::FerriteWindow;
use super::drop::{Carried, DragSession, escape_cancels, keys_of};
use super::drop_platform::native_handle;
use crate::drag::{DragState, FileDrag};
use crate::prelude::*;

#[derive(Clone)]
pub(super) struct NativeDrop {
    pub(super) keys: DropKeys,
    pub(super) right: bool,
    allowed: [bool; 3],
}

impl NativeDrop {
    pub(super) fn allows(&self, op: DropOp) -> bool {
        self.allowed[match op {
            DropOp::Copy => 0,
            DropOp::Move => 1,
            DropOp::Link => 2,
        }]
    }
}

impl FerriteWindow {
    fn native_drop(&self, window: &Window) -> NativeDrop {
        let handle = native_handle(window);
        let platform = self.drag.platform.as_ref();
        NativeDrop {
            keys: platform
                .and_then(|platform| platform.keys(handle))
                .unwrap_or_else(|| keys_of(&window.modifiers())),
            right: platform.is_some_and(|platform| platform.right_button(handle)),
            allowed: [DropOp::Copy, DropOp::Move, DropOp::Link]
                .map(|op| platform.is_none_or(|platform| platform.allows(handle, op))),
        }
    }

    pub(super) fn set_drop_effect(&self, op: Option<DropOp>, window: &Window) {
        if let Some(platform) = &self.drag.platform {
            platform.set_effect(native_handle(window), op);
        }
    }

    pub(super) fn drops_blocked(&self) -> bool {
        self.browser.pending().is_some()
            || self.prompt.is_some()
            || self.is_multi_rename_open()
            || self.settings.is_open()
            || self.drag.menu.is_some()
            || self.viewer.is_some()
            || self.menu_run.is_some()
            || self.app_menu.is_some()
            || self.path_edit.is_some()
    }

    pub(super) fn external_drops_on(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_drag_move(
            cx.listener(|view, event: &DragMoveEvent<ExternalPaths>, window, cx| {
                let paths = event.drag(cx).clone();
                view.enter_external(paths, window, cx);
                view.update_native_drop(window);
            }),
        )
        .on_drag_move(
            cx.listener(|view, _: &DragMoveEvent<FileDrag>, window, cx| {
                let Some(session) = view
                    .drag
                    .session
                    .as_mut()
                    .filter(|session| session.external.is_none())
                else {
                    return;
                };
                if session.outgoing || session.native.is_some() {
                    session.outgoing = false;
                    session.native = None;
                    session.shown = None;
                    view.set_drop_effect(None, window);
                    cx.notify();
                }
            }),
        )
    }

    fn enter_external(&mut self, paths: ExternalPaths, window: &Window, cx: &mut Context<Self>) {
        if self
            .drag
            .session
            .as_ref()
            .is_some_and(|session| session.external.as_ref() == Some(&paths))
        {
            return;
        }
        let carried = self.external_carried(&paths);
        let error = carried.as_ref().err().cloned();
        let carried = carried.unwrap_or_else(|_| Carried {
            sources: Vec::new(),
            parents: Vec::new(),
            folders: Vec::new(),
            first: SharedString::default(),
            tab: None,
        });
        let state = cx.new(|_| DragState {
            count: carried.sources.len(),
            first_name: carried.first.clone(),
            hover: None,
            keys_line: false,
        });
        self.drag.session = Some(DragSession {
            carried,
            source_pane: self.browser.state().focused_pane(),
            state,
            over: None,
            keys: DropKeys::default(),
            shown: None,
            shown_target: None,
            shown_tab: None,
            external: Some(paths),
            outgoing: false,
            native: None,
            error,
            spring: super::spring::Spring::default(),
            _escape: escape_cancels(window, cx),
        });
    }

    fn external_carried(&self, paths: &ExternalPaths) -> Result<Carried, SharedString> {
        let sources = self
            .drag
            .platform
            .as_ref()
            .ok_or_else(|| rusty_l10n::tr("drag-refused-platform"))?
            .locations(&paths.0)
            .map_err(SharedString::from)?;
        if sources.is_empty() || sources.len() != paths.0.len() {
            return Err(rusty_l10n::tr("drag-refused-platform").into());
        }
        Ok(Carried {
            parents: sources.iter().filter_map(Location::parent).collect(),
            folders: sources.clone(),
            first: sources
                .first()
                .map(|source| source.as_str().to_owned().into())
                .unwrap_or_default(),
            sources,
            tab: None,
        })
    }

    fn update_native_drop(&mut self, window: &Window) {
        let mut native = self.native_drop(window);
        self.set_drop_effect(None, window);
        if let Some(session) = &mut self.drag.session {
            native.right |= session.native.as_ref().is_some_and(|native| native.right);
            session.native = Some(native);
            session.shown = None;
        }
    }

    pub(super) fn drag_out_payload(&mut self, drag: &FileDrag) -> Option<ExternalDragPayload> {
        if self.drops_blocked() {
            return None;
        }
        let session = self
            .drag
            .session
            .as_mut()
            .filter(|session| session.source_pane == drag.pane)?;
        let items: Vec<_> = session
            .carried
            .sources
            .iter()
            .map(|source| (source.clone(), session.carried.folders.contains(source)))
            .collect();
        let paths = self.drag.platform.as_ref()?.paths(&items)?;
        if paths.is_empty() || paths.len() != items.len() {
            return None;
        }
        session.outgoing = true;
        session.spring.stop();
        Some(ExternalDragPayload::Files(FileDragPaths::new(paths)))
    }

    pub(super) fn drop_lifecycle(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.weak_entity();
        gpui_kit::canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                Self::listen_drop_lifecycle(view.clone(), window);
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn listen_drop_lifecycle(view: gpui_kit::WeakEntity<Self>, window: &mut Window) {
        let lifecycle = view.clone();
        window.on_mouse_event(move |event: &FileDropEvent, phase, _, cx| {
            if phase != DispatchPhase::Capture {
                return;
            }
            let _ = lifecycle.update(cx, |view, cx| view.native_drag_event(event, cx));
        });
        window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture {
                return;
            }
            let _ = view.update(cx, |view, _| view.before_drag_release(window));
            let release = view.clone();
            window.defer(cx, move |window, cx| {
                let _ = release.update(cx, |view, cx| view.after_drag_release(window, cx));
            });
        });
    }

    fn native_drag_event(&mut self, event: &FileDropEvent, cx: &mut Context<Self>) {
        match event {
            FileDropEvent::Ended => {
                let _ = self.finish_native_drag();
            }
            FileDropEvent::Exited
                if self
                    .drag
                    .session
                    .as_ref()
                    .is_some_and(|session| session.external.is_some()) =>
            {
                let _ = self.end_drag();
            }
            _ => return,
        }
        cx.notify();
    }

    fn finish_native_drag(&mut self) -> Option<DragSession> {
        let session = self.end_drag()?;
        if session.outgoing {
            self.browser.refresh_unwatched(&session.carried.parents);
        }
        Some(session)
    }

    fn before_drag_release(&mut self, window: &Window) {
        if self
            .drag
            .session
            .as_ref()
            .is_some_and(|session| session.native.is_some())
        {
            self.update_native_drop(window);
        }
    }

    fn after_drag_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.has_active_drag() {
            if let Some(session) = self.finish_native_drag()
                && session.external.is_none()
                && !session.outgoing
            {
                self.focus_pane(session.source_pane);
                self.focus_core_pane(window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn drag_layer(&self, root: Div, cx: &mut Context<Self>) -> Div {
        Self::external_drops_on(
            root.children(self.drop_menu_view())
                .child(self.drop_lifecycle(cx)),
            cx,
        )
    }
}
