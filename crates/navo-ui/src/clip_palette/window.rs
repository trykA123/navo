//! The pop-up palette window over another app: one per app, made on the first clipboard
//! summon and hidden, not closed, afterwards. Enter pastes into the app that was in front;
//! Esc or focus loss hides it.

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, DisplayId, Entity, Focusable, Global,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window, WindowBounds,
    WindowKind, WindowOptions, div, px, size,
};

use super::ClipboardHooks;
use super::view::{ClipPalette, ClipPaletteEvent, ClipPaletteProps, Place};
use crate::window_view::{ClipboardSummon, PasteTarget, WindowShown, raw_handle};

const WIDTH: f32 = 720.;
const HEIGHT: f32 = 400.;

/// What a clipboard summon needs to show the pop-up.
#[derive(Clone)]
pub struct PopUpRequest {
    pub summon: ClipboardSummon,
    pub hooks: Arc<dyn ClipboardHooks>,
    pub key_cap: Option<SharedString>,
    /// Hides and shows the native window; without it, hiding closes the window.
    pub window_shown: Option<WindowShown>,
}

#[derive(Default)]
struct Slot(Option<(AnyWindowHandle, Entity<ClipPopUp>)>);

impl Global for Slot {}

/// Shows the app's pop-up, making it on the first summon or when the cursor is on another
/// display.
pub fn summon(request: PopUpRequest, cx: &mut App) {
    if let Some((handle, pop_up)) = cx.default_global::<Slot>().0.clone() {
        let shown = handle.update(cx, |_, window, cx| {
            pop_up.update(cx, |pop_up, cx| pop_up.show(&request, window, cx))
        });
        if shown.unwrap_or(false) {
            return;
        }
    }
    open(request, cx);
}

fn open(request: PopUpRequest, cx: &mut App) {
    let display = request
        .summon
        .display
        .map(DisplayId::new)
        .filter(|id| cx.find_display(*id).is_some());
    let bounds = Bounds::centered(display, size(px(WIDTH), px(HEIGHT)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: display,
        app_id: Some(navo_core::brand::SLUG.into()),
        ..WindowOptions::default()
    };
    let opened = crate::open_window(options, cx, |window, cx| {
        cx.new(|cx| ClipPopUp::new(request, window, cx))
    });
    match opened {
        Ok(made) => cx.set_global(Slot(Some(made))),
        Err(_) => tracing::warn!("the clipboard pop-up window did not open"),
    }
}

/// The open pop-up, if one was made.
#[cfg(test)]
pub(crate) fn current(cx: &App) -> Option<(AnyWindowHandle, Entity<ClipPopUp>)> {
    cx.try_global::<Slot>().and_then(|slot| slot.0.clone())
}

fn log_summon(at: Instant, window: &Window) {
    window.on_next_frame(move |_, _| {
        let _span = tracing::debug_span!("clipboard.summon").entered();
        let summon_ms = u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX);
        tracing::info!(summon_ms, "clipboard pop-up shown");
    });
}

pub(crate) struct ClipPopUp {
    palette: Entity<ClipPalette>,
    previous: Option<Arc<dyn PasteTarget>>,
    window_shown: Option<WindowShown>,
    is_shown: bool,
    /// Set once the window was active after a show, so only a real focus loss hides it.
    was_active: bool,
    _watch: [Subscription; 2],
}

impl ClipPopUp {
    fn new(request: PopUpRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let props = ClipPaletteProps {
            hooks: request.hooks,
            place: Place::Anywhere,
            folder: None,
            key_cap: request.key_cap,
            now_ms: None,
            width: None,
        };
        let palette = cx.new(|cx| ClipPalette::new(props, window, cx));
        let events = cx.subscribe_in(&palette, window, |this, _, event, window, cx| {
            this.palette_event(event, window, cx);
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.activation_changed(window, cx);
        });
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |pop_up, cx| !pop_up.conceal(window, cx))
                .unwrap_or(true)
        });
        log_summon(request.summon.at, window);
        Self {
            palette,
            previous: request.summon.previous,
            window_shown: request.window_shown,
            is_shown: true,
            was_active: window.is_window_active(),
            _watch: [events, activation],
        }
    }

    /// False when the window was closed because the cursor is on another display.
    fn show(
        &mut self,
        request: &PopUpRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let here = window.display(cx).map(|display| display.id());
        if let Some(wanted) = request.summon.display
            && here.is_some_and(|here| here != DisplayId::new(wanted))
        {
            self.previous = None;
            self.palette
                .update(cx, |palette, cx| palette.reset(window, cx));
            window.remove_window();
            return false;
        }
        self.previous.clone_from(&request.summon.previous);
        self.window_shown.clone_from(&request.window_shown);
        self.palette.update(cx, |palette, cx| palette.wake(cx));
        if !self.is_shown {
            self.is_shown = true;
            self.was_active = false;
            if let Some(hook) = &self.window_shown {
                hook(raw_handle(window), true);
            }
        }
        window.activate_window();
        window.focus(&self.palette.focus_handle(cx), cx);
        log_summon(request.summon.at, window);
        true
    }

    fn palette_event(
        &mut self,
        event: &ClipPaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ClipPaletteEvent::Dismissed => self.hide(window, cx),
            ClipPaletteEvent::PasteBack => self.paste_back(window, cx),
            ClipPaletteEvent::GoTo(_) | ClipPaletteEvent::PasteIntoPane => {}
        }
    }

    fn activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            self.was_active = true;
        } else if self.was_active {
            self.hide(window, cx);
        }
    }

    /// The item is already on the clipboard: hide, then paste into the app that was in front.
    /// Navo's own window only gets the copy.
    fn paste_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.previous.take();
        self.hide(window, cx);
        let Some(target) = target.filter(|target| !target.is_own()) else {
            return;
        };
        cx.background_spawn(async move {
            if let Err(error) = target.paste() {
                tracing::warn!(%error, "pasting back into the previous window failed");
            }
        })
        .detach();
    }

    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_shown && !self.conceal(window, cx) {
            window.remove_window();
        }
    }

    /// Forgets the paste target and the search, then hides the native window; false when
    /// there is no way to hide it.
    fn conceal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.is_shown = false;
        self.was_active = false;
        self.previous = None;
        self.palette
            .update(cx, |palette, cx| palette.reset(window, cx));
        self.window_shown
            .as_ref()
            .is_some_and(|hook| hook(raw_handle(window), false))
    }

    #[cfg(test)]
    pub(crate) fn is_shown(&self) -> bool {
        self.is_shown
    }

    #[cfg(test)]
    pub(crate) fn has_rows(&self, cx: &gpui_kit::App) -> bool {
        self.palette.read(cx).state.selected_row().is_some()
    }

    #[cfg(test)]
    pub(crate) fn has_target(&self) -> bool {
        self.previous.is_some()
    }
}

impl Render for ClipPopUp {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.palette.clone())
    }
}

impl ClipPalette {
    /// Back to an empty search with no menu, status, shown secret or rows, until `wake`.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay.menu = None;
        self.overlay.status = None;
        self.overlay.revealed = None;
        self.manage = None;
        self.intro = false;
        self.dormant = true;
        self.state.clear();
        self.field
            .update(cx, |field, cx| field.set_value(String::new(), window, cx));
        cx.notify();
    }

    /// Searches again after a `reset`, for the next summon.
    fn wake(&mut self, cx: &mut Context<Self>) {
        self.dormant = false;
        self.refresh(false, cx);
    }
}
