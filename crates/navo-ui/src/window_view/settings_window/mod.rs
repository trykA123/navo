//! Owned floating Settings window host: lifecycle, event routing and book ownership.

mod view;

use std::rc::Rc;
use std::sync::OnceLock;

use gpui_kit::{Entity, WeakEntity, WindowHandle, WindowKind};

use super::hand_off::RawWindowHandle;
use super::{NavoWindow, raw_handle};
use crate::prelude::*;
use crate::settings::SheetEvent;
use view::SettingsWindow;

/// Smallest logical width of the Settings window.
const MIN_W: f32 = 360.0;
/// Smallest logical height of the Settings window.
const MIN_H: f32 = 420.0;

/// Owns the floating Settings window handle and its hidden bookkeeping.
#[derive(Default)]
pub(in crate::window_view) struct SettingsHost {
    window: Option<WindowHandle<SettingsWindow>>,
    handle: Option<AnyWindowHandle>,
    main: Option<WindowHandle<NavoWindow>>,
    raw: Option<RawWindowHandle>,
    user_hidden: bool,
    follow_hidden: bool,
}

impl SettingsHost {
    /// Records the freshly opened Settings window and marks it shown.
    pub(in crate::window_view) fn open(
        &mut self,
        handle: AnyWindowHandle,
        main: WindowHandle<NavoWindow>,
        raw: Option<RawWindowHandle>,
    ) {
        self.window = Some(WindowHandle::<SettingsWindow>::new(handle.window_id()));
        self.handle = Some(handle);
        self.main = Some(main);
        self.raw = raw;
        self.user_hidden = false;
    }

    /// Returns Navo's window handle while the Settings window is open.
    pub(in crate::window_view) fn main(&self) -> Option<WindowHandle<NavoWindow>> {
        self.main
    }

    /// Shows and activates an already-open Settings window; false when none is live.
    pub(in crate::window_view) fn show(&mut self, cx: &mut App) -> bool {
        let shown = self.focus(cx);
        if shown {
            self.user_hidden = false;
        }
        shown
    }

    /// Activates the tracked Settings window; clears stale state when it is gone.
    pub(in crate::window_view) fn focus(&mut self, cx: &mut App) -> bool {
        let Some(handle) = self.window else {
            return false;
        };
        let active = cx
            .update_window(handle.into(), |_, window, _| window.activate_window())
            .is_ok();
        if !active {
            self.clear();
        }
        active
    }

    /// Runs `f` on the Settings window for window-bound entities and focus; None when closed.
    pub(in crate::window_view) fn with_window<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        let handle = self.window?;
        cx.update_window(handle.into(), |_, window, cx| f(window, cx))
            .ok()
    }

    /// Runs `f` on the Settings window when open and not already active as `fallback`,
    /// else falls back to `fallback` (re-entrancy guard and overlay mode).
    pub(in crate::window_view) fn with_window_or<R>(
        &self,
        fallback: &mut Window,
        cx: &mut App,
        mut f: impl FnMut(&mut Window, &mut App) -> R,
    ) -> R {
        if let Some(handle) = self.window
            && fallback.window_handle().window_id() != handle.window_id()
            && let Some(result) = self.with_window(cx, |window, cx| f(window, cx))
        {
            return result;
        }
        f(fallback, cx)
    }

    /// Closes the Settings window and clears all tracked state.
    pub(in crate::window_view) fn close(&mut self, cx: &mut App) {
        if let Some(handle) = self.window {
            let _ = cx.update_window(handle.into(), |_, window, _| window.remove_window());
        }
        self.clear();
    }

    /// Returns the live Settings window handle, if currently open.
    pub(in crate::window_view) fn window(&self) -> Option<WindowHandle<SettingsWindow>> {
        self.window
    }

    /// Returns the live Settings window as an untyped handle, if currently open.
    pub(in crate::window_view) fn window_handle(&self) -> Option<AnyWindowHandle> {
        self.handle
    }

    /// Returns the native raw handle captured when the Settings window opened.
    pub(in crate::window_view) fn raw(&self) -> Option<RawWindowHandle> {
        self.raw
    }

    /// Returns whether a Settings window is currently tracked as open.
    pub(in crate::window_view) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    /// Returns whether the user hid the Settings window instead of closing it.
    pub(in crate::window_view) fn is_hidden(&self) -> bool {
        self.user_hidden
    }

    /// Updates whether the user hid the Settings window.
    pub(in crate::window_view) fn set_hidden(&mut self, hidden: bool) {
        self.user_hidden = hidden;
    }

    /// Returns whether the Settings window follows Navo into the tray.
    pub(in crate::window_view) fn follow_hidden(&self) -> bool {
        self.follow_hidden
    }

    /// Updates whether the Settings window follows Navo into the tray.
    pub(in crate::window_view) fn set_follow_hidden(&mut self, hidden: bool) {
        self.follow_hidden = hidden;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Whether the Settings window replaces the overlay: always in tests, else `NAVO_SETTINGS_WINDOW=1`.
pub(in crate::window_view) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        cfg!(test) || std::env::var_os("NAVO_SETTINGS_WINDOW").is_some_and(|v| v == "1")
    })
}

/// Runs `f` on Navo's view inside Navo's own window. `gpui_kit::open_window` wraps the root
/// in `base::Root`, so the typed handle resolves through the root's content view.
pub(super) fn update_navo<R>(
    main: &WindowHandle<NavoWindow>,
    cx: &mut App,
    f: impl FnOnce(&mut NavoWindow, &mut Window, &mut Context<NavoWindow>) -> R,
) -> Option<R> {
    cx.update_window((*main).into(), |_, window, cx| {
        let Some(Some(root)) = window.root::<gpui_kit::base::Root>() else {
            return None;
        };
        let view = root.read(cx).view().clone().downcast::<NavoWindow>().ok()?;
        Some(view.update(cx, |navo, cx| f(navo, window, cx)))
    })
    .ok()?
}

type SettingsEventHandler<E> =
    Rc<dyn Fn(&mut NavoWindow, &E, &mut Window, &mut Context<NavoWindow>)>;
type SettingsActionHandler = Rc<dyn Fn(&mut NavoWindow, &mut Window, &mut Context<NavoWindow>)>;

/// Runs one cloned Settings event on Navo's window; warns when Navo is already gone.
fn deliver_event<E>(
    main: WindowHandle<NavoWindow>,
    event: E,
    f: SettingsEventHandler<E>,
    cx: &mut App,
) {
    let delivered = update_navo(&main, cx, |navo, window, cx| f(navo, &event, window, cx));
    if delivered.is_none() {
        tracing::warn!("a settings event arrived after Navo's window closed");
    }
}

/// Runs one Settings action or click handler on Navo's window; warns when Navo is gone.
fn deliver_action(main: WindowHandle<NavoWindow>, f: SettingsActionHandler, cx: &mut App) {
    let delivered = update_navo(&main, cx, |navo, window, cx| f(navo, window, cx));
    if delivered.is_none() {
        tracing::warn!("a settings action arrived after Navo's window closed");
    }
}

/// Routing target for Settings actions and key listeners: routes onto `main` when Settings
/// runs in its own floating window, else runs directly on `navo` in overlay mode.
#[derive(Clone)]
pub(in crate::window_view) struct SettingsRoute {
    pub(in crate::window_view) navo: WeakEntity<NavoWindow>,
    pub(in crate::window_view) main: Option<WindowHandle<NavoWindow>>,
}

impl SettingsRoute {
    /// Builds a handler that stops propagation synchronously and routes onto Navo's window
    /// when Settings runs in its own window, else runs directly on the overlay window.
    pub(in crate::window_view) fn listener_stopping<E: Clone + 'static>(
        &self,
        f: impl Fn(&mut NavoWindow, &E, &mut Window, &mut Context<NavoWindow>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let main = self.main;
        let this = self.navo.clone();
        let f: SettingsEventHandler<E> = Rc::new(f);
        move |event: &E, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            let f = Rc::clone(&f);
            if let Some(main) = main {
                let event = event.clone();
                cx.defer(move |cx| deliver_event(main, event, f, cx));
            } else {
                let _ = this.update(cx, |navo, cx| f(navo, event, window, cx));
            }
        }
    }

    /// Builds an action or click handler that routes onto Navo's window when Settings runs
    /// in its own window, else runs directly on the overlay window.
    pub(in crate::window_view) fn action<E: 'static>(
        &self,
        f: impl Fn(&mut NavoWindow, &mut Window, &mut Context<NavoWindow>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let main = self.main;
        let this = self.navo.clone();
        let f: SettingsActionHandler = Rc::new(f);
        move |_: &E, window: &mut Window, cx: &mut App| {
            let f = Rc::clone(&f);
            if let Some(main) = main {
                cx.defer(move |cx| deliver_action(main, f, cx));
            } else {
                let _ = this.update(cx, |navo, cx| f(navo, window, cx));
            }
        }
    }
}

impl NavoWindow {
    /// Returns the active routing target for Settings controls built from this window.
    pub(in crate::window_view) fn settings_route(&self, cx: &Context<Self>) -> SettingsRoute {
        SettingsRoute {
            navo: cx.weak_entity(),
            main: self.settings.host.main(),
        }
    }

    /// Builds an `on_event` handler that routes one cloned event onto Navo's own window:
    /// propagation stops synchronously, then the cloned event runs deferred on Navo.
    pub(in crate::window_view) fn settings_listener<E: Clone + 'static>(
        main: WindowHandle<NavoWindow>,
        f: impl Fn(&mut Self, &E, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let f: SettingsEventHandler<E> = Rc::new(f);
        move |event: &E, _window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            let event = event.clone();
            let f = Rc::clone(&f);
            cx.defer(move |cx| deliver_event(main, event, f, cx));
        }
    }

    /// Opens the floating Settings window, or shows and focuses the existing one.
    pub(in crate::window_view) fn open_settings_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.host.show(cx) {
            return;
        }
        let navo = cx.weak_entity();
        let main = WindowHandle::<NavoWindow>::new(window.window_handle().window_id());
        let navo_raw = native_raw(window);
        // gpui draws a new window once before open_window returns; that draw renders
        // SettingsWindow, which reads Navo's settings state. Defer the open so the draw
        // runs outside this NavoWindow update lease.
        cx.defer(move |cx| {
            match navo.update(cx, |navo, cx| navo.settings.host.show(cx)) {
                Ok(false) => {}
                Ok(true) | Err(_) => return,
            }
            let Some((handle, view, settings_raw)) = open_the_window(navo.clone(), main, cx) else {
                let _ = navo.update(cx, |navo, cx| navo.reset_settings_on_close(true, cx));
                return;
            };
            if update_navo(&main, cx, |navo, window, cx| {
                navo.settings.host.open(handle, main, settings_raw);
                navo.check_settings_owner(settings_raw, navo_raw);
                navo.prepare_settings_window(window, cx);
                cx.notify();
            })
            .is_none()
            {
                let _ = cx.update_window(handle, |_, window, _| window.remove_window());
                return;
            }
            let snapshot = navo
                .upgrade()
                .map(|entity| entity.read(cx).settings_snapshot());
            view.update(cx, |settings, cx| settings.set_snapshot(snapshot, cx));
            focus_in_settings(navo, handle, cx);
        });
    }

    /// Closes the floating Settings window, dropping window-bound inputs and activating Navo.
    pub(in crate::window_view) fn close_settings_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.host.is_open() {
            return;
        }
        self.close_settings(window, cx);
    }

    /// Clears SettingsHost and window-bound inputs when the Settings window is closing itself.
    fn on_settings_window_closing(
        &mut self,
        was_active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings.host.is_open() {
            return;
        }
        self.reset_settings_on_close(true, cx);
        if was_active {
            window.activate_window();
        }
        self.settings.host.clear();
        self.focus_after_settings(window, cx);
        cx.notify();
    }

    /// Handles one routed Settings event on Navo's window; Close runs the escape chain first.
    fn settings_window_event(
        &mut self,
        event: &SheetEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_event(event.clone(), window, cx);
    }

    /// Checks the native owner right after the Settings window opened.
    fn check_settings_owner(
        &self,
        settings_raw: Option<RawWindowHandle>,
        navo_raw: Option<RawWindowHandle>,
    ) {
        let (Some(settings_raw), Some(navo_raw)) = (settings_raw, navo_raw) else {
            return;
        };
        if (self.find.hooks().ensure_owner)(settings_raw, navo_raw) {
            tracing::warn!("the settings window owner had to be repaired after open");
        }
    }
}

/// Focuses Navo's settings content inside the given Settings window.
fn focus_in_settings(this: WeakEntity<NavoWindow>, handle: AnyWindowHandle, cx: &mut App) {
    let _ = cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        let Some(navo) = this.upgrade() else { return };
        navo.update(cx, |navo, cx| navo.focus_settings(window, cx));
    });
}

/// Opens the floating Settings window and returns its handle, view entity and native raw handle.
fn open_the_window(
    navo: WeakEntity<NavoWindow>,
    main: WindowHandle<NavoWindow>,
    cx: &mut App,
) -> Option<(
    AnyWindowHandle,
    Entity<SettingsWindow>,
    Option<RawWindowHandle>,
)> {
    let initial = navo
        .upgrade()
        .map(|entity| entity.read(cx).settings_snapshot());
    let options = WindowOptions {
        kind: WindowKind::Floating,
        is_minimizable: false,
        window_min_size: Some(size(px(MIN_W), px(MIN_H))),
        app_id: Some(navo_core::brand::SLUG.into()),
        ..crate::title_bar::window_options(navo_l10n::t!("settings-title"))
    };
    let mut settings_raw = None;
    let opened = crate::open_window(options, cx, |settings_window, cx| {
        settings_raw = native_raw(settings_window);
        let view = cx.new(|cx| SettingsWindow::new(navo, main, initial, cx));
        settings_window.on_window_should_close(cx, move |settings_window, cx| {
            let was_active = settings_window.is_window_active();
            if update_navo(&main, cx, |navo, window, cx| {
                navo.on_settings_window_closing(was_active, window, cx);
            })
            .is_none()
            {
                tracing::warn!("a settings close arrived after Navo's window closed");
            }
            true
        });
        view
    });
    match opened {
        Ok((handle, view)) => Some((handle, view, settings_raw)),
        Err(error) => {
            tracing::warn!(%error, "the settings window did not open");
            None
        }
    }
}

/// The window's native raw handle; test builds fall back to a synthetic handle per window.
fn native_raw(window: &Window) -> Option<RawWindowHandle> {
    let raw = raw_handle(window);
    #[cfg(test)]
    let raw = raw.or_else(|| Some(super::find_window::test_raw_handle(window.window_handle())));
    raw
}

// Pins host accessors and entry points until the opener, minimise and tray atoms wire them.
const _: fn() -> bool = on;
const _: fn(&mut NavoWindow, &mut Window, &mut Context<NavoWindow>) =
    NavoWindow::open_settings_window;
const _: fn(&SettingsHost) -> Option<WindowHandle<SettingsWindow>> = SettingsHost::window;
const _: fn(&SettingsHost) -> Option<AnyWindowHandle> = SettingsHost::window_handle;
const _: fn(&SettingsHost) -> Option<RawWindowHandle> = SettingsHost::raw;
const _: fn(&SettingsHost) -> bool = SettingsHost::is_hidden;
const _: fn(&mut SettingsHost, bool) = SettingsHost::set_hidden;
const _: fn(&SettingsHost) -> bool = SettingsHost::follow_hidden;
const _: fn(&mut SettingsHost, bool) = SettingsHost::set_follow_hidden;
