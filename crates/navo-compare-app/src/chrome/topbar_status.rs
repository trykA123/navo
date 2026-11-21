//! The topbar (NC66): a 52 px bar of mode/screen crumbs and action pills above the session.
//! Everything shrinks with `min_w_0` so the bar never clips down to the 780 px minimum width.

use std::rc::Rc;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{FocusHandle, KeyBinding, Subscription};
use sidewise_kit::prelude::*;
use sidewise_kit::primitives::{TextFieldState, text_field_state};

gpui_kit::actions!(topbar, [ViewMenu, SyncPlan, FocusSearch]);

/// Key context of the topbar; its action keys are bound inside it.
const CONTEXT: &str = "Topbar";
/// The topbar height of the prototype.
const TOP_H: f32 = 52.;
/// The narrowest window the topbar must fit without horizontal clipping.
pub const MIN_W: f32 = 780.;
/// Width below which the topbar uses compact spacing and omits pill key hints.
const COMPACT_W: f32 = 640.;
/// Viewport width fallback used before the first prepaint measures the topbar directly.
const COMPACT_VIEWPORT_W: f32 = 920.;
/// The height of one action pill.
const PILL_H: f32 = 28.;

/// Binds the topbar's action keys in the `Topbar` context.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("alt-v", ViewMenu, Some(CONTEXT)),
        KeyBinding::new("alt-s", SyncPlan, Some(CONTEXT)),
        // Skip `/` while an `Input` owns focus so users can type path separators in filters.
        KeyBinding::new("/", FocusSearch, Some("Topbar && !Input")),
    ]);
}

#[path = "topbar_menu.rs"]
mod menu;
#[path = "topbar_mode.rs"]
mod mode;
#[path = "topbar_view.rs"]
mod view;

pub use mode::{Action, TopbarMode};

/// The callback every topbar action fires, on the UI thread.
type OnAction = Rc<dyn Fn(Action, &mut App)>;

/// The callback fired whenever user typing or clearing edits the filter text.
type OnFilterChange = Rc<dyn Fn(SharedString, &mut App)>;

/// Breadcrumb strings shown on the left side of the topbar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumbs {
    pub title: SharedString,
    pub detail: SharedString,
}

/// Connection state shown by the Accounts indicator from caller state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccountConnection {
    #[default]
    Disconnected,
    #[expect(dead_code, reason = "the Accounts pill is hidden in alpha")]
    Connected,
}

/// The topbar: its mode, crumbs, filter text, availability, and callback.
pub struct Topbar {
    mode: TopbarMode,
    enabled: [bool; 6],
    compact: bool,
    title: SharedString,
    detail: SharedString,
    filter: SharedString,
    filter_input: Entity<TextFieldState>,
    _filter_sub: Subscription,
    connection: AccountConnection,
    on_action: OnAction,
    on_filter_change: Option<OnFilterChange>,
    handle: FocusHandle,
    menu: Option<(Action, gpui_kit::AnyView)>,
}

impl Topbar {
    /// A topbar over one crumb pair, firing `on_action` for every pill and action key.
    pub fn new(
        crumbs: Crumbs,
        on_action: impl Fn(Action, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter_input = text_field_state("", window, cx);
        let _filter_sub = cx.subscribe_in(
            &filter_input,
            window,
            |this, input, event: &InputEvent, _, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let text = input.read(cx).value();
                this.on_filter_input(text, cx);
            },
        );
        Self {
            mode: TopbarMode::default(),
            enabled: [true; 6],
            compact: window.viewport_size().width < px(COMPACT_VIEWPORT_W),
            title: crumbs.title,
            detail: crumbs.detail,
            filter: SharedString::default(),
            filter_input,
            _filter_sub,
            connection: AccountConnection::Disconnected,
            on_action: Rc::new(on_action),
            on_filter_change: None,
            handle: cx.focus_handle(),
            menu: None,
        }
    }

    /// Switches which screen controls the topbar mounts.
    pub fn set_mode(&mut self, mode: TopbarMode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        cx.notify();
    }

    /// The screen mode now mounted; for tests.
    #[cfg(test)]
    pub(crate) fn mode(&self) -> TopbarMode {
        self.mode
    }

    /// Enables or disables one action across pills, shortcut handlers, and search focus.
    pub fn set_available(&mut self, action: Action, available: bool, cx: &mut Context<Self>) {
        self.enabled[action as usize] = available;
        cx.notify();
    }

    /// Updates the displayed breadcrumb title and detail strings; redraws only on a change.
    pub fn set_crumbs(&mut self, crumbs: Crumbs, cx: &mut Context<Self>) {
        if self.title == crumbs.title && self.detail == crumbs.detail {
            return;
        }
        self.title = crumbs.title;
        self.detail = crumbs.detail;
        cx.notify();
    }

    /// Returns whether `action` is both mounted in the current mode and enabled.
    pub fn is_available(&self, action: Action) -> bool {
        self.mode.shows(action) && self.enabled[action as usize]
    }

    /// Updates compact mode from observed own width, notifying only on a transition.
    fn record_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let next = width < px(COMPACT_W);
        if self.compact != next {
            self.compact = next;
            crate::frame_notify::notify_after_frame(cx);
        }
    }

    /// Updates the Accounts connection indicator state from the caller.
    #[expect(dead_code, reason = "the Accounts pill is hidden in alpha")]
    pub fn set_connection(&mut self, state: AccountConnection, cx: &mut Context<Self>) {
        self.connection = state;
        cx.notify();
    }

    /// Registers the callback invoked whenever user editing changes the filter text.
    pub fn set_on_filter_change(&mut self, callback: impl Fn(SharedString, &mut App) + 'static) {
        self.on_filter_change = Some(Rc::new(callback));
    }

    /// The current search filter text.
    pub fn filter(&self) -> &SharedString {
        &self.filter
    }

    /// The focus handle, so the shell can focus the bar before its action keys are used.
    pub fn focus_handle(&self) -> FocusHandle {
        self.handle.clone()
    }

    /// Replaces the search filter text and syncs the editable field.
    pub fn set_filter(
        &mut self,
        text: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        self.filter = text.clone();
        self.filter_input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        cx.notify();
    }

    /// Syncs `filter` from user input edits and notifies any registered change listener.
    fn on_filter_input(&mut self, text: SharedString, cx: &mut Context<Self>) {
        self.filter = text.clone();
        if let Some(callback) = &self.on_filter_change {
            callback(text, cx);
        }
        cx.notify();
    }

    /// Focuses the inline search field and notifies the caller when Search is available.
    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_available(Action::Search) {
            return;
        }
        self.filter_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.fire(Action::Search, cx);
    }

    /// Runs the registered callback for one action when it is shown and enabled.
    fn fire(&self, action: Action, cx: &mut App) {
        if !self.is_available(action) {
            return;
        }
        (self.on_action)(action, cx);
    }
}
