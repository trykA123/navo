//! Bottom terminal drawer view, module-off notice overlay, and title-bar Terminal button.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    FocusHandle, KeyDownEvent, Keystroke, MouseButton, RenderOnce, Role, Subscription, Task, canvas,
};

use super::grid::{GridStyle, TerminalGrid};
use super::input::{encode_key, encode_paste};
use super::session::TerminalSession;
use super::{RowCache, grid_dimensions, measure_cell};
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName, Tooltip, icon};
use crate::tokens;

/// Default bottom drawer height as a percentage of the window height.
pub const DEFAULT_DRAWER_HEIGHT_PCT: u16 = 40;
/// Minimum bottom drawer height in logical pixels.
pub const MIN_DRAWER_HEIGHT: Pixels = px(168.0);
/// Keymap context for a focused terminal view.
pub const CONTEXT: &str = "Terminal";

/// Debounce delay before sending a PTY resize after layout bounds settle (R4).
const RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);
const HEADER_ICON_SIZE: f32 = 14.0;
const NOTICE_ICON_SIZE: f32 = 18.0;
const NOTICE_MAX_WIDTH: f32 = 440.0;

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Bottom terminal drawer view bound to one [`TerminalSession`] entity (D161.3).
pub struct TerminalDrawer {
    session: Entity<TerminalSession>,
    cache: Entity<RowCache>,
    focus: FocusHandle,
    measured_dims: Rc<Cell<(u16, u16)>>,
    pending_dims: (u16, u16),
    resize_task: Option<Task<()>>,
    on_hide: Option<Callback>,
    _session_sub: Subscription,
}

impl TerminalDrawer {
    /// Creates a drawer view observing `session`.
    pub fn new(
        session: Entity<TerminalSession>,
        on_hide: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let initial = session.read(cx).size();
        let cache = cx.new(|_| RowCache::new());
        let focus = cx.focus_handle();
        let session_sub = cx.observe(&session, |_, _, cx| cx.notify());
        Self {
            session,
            cache,
            focus,
            measured_dims: Rc::new(Cell::new((initial.cols, initial.rows))),
            pending_dims: (initial.cols, initial.rows),
            resize_task: None,
            on_hide: Some(Rc::new(on_hide)),
            _session_sub: session_sub,
        }
    }

    /// Focus handle for the drawer's interactive terminal surface.
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Returns whether the drawer currently holds keyboard focus in `window`.
    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_paste_shortcut(&event.keystroke) {
            self.paste_clipboard(cx);
            cx.stop_propagation();
            return;
        }
        let modes = self.session.read(cx).modes();
        if let Some(bytes) = encode_key(&event.keystroke, modes) {
            self.session.update(cx, |session, _| {
                let _ = session.send_bytes(&bytes);
            });
            cx.stop_propagation();
        }
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let modes = self.session.read(cx).modes();
        let bytes = encode_paste(&text, modes);
        self.session.update(cx, |session, _| {
            let _ = session.send_bytes(&bytes);
        });
    }

    fn sync_measured_size(&mut self, cx: &mut Context<Self>) {
        let dims = self.measured_dims.get();
        if dims.0 == 0 || dims.1 == 0 || dims == self.pending_dims {
            return;
        }
        self.pending_dims = dims;
        let session = self.session.clone();
        self.resize_task = Some(cx.spawn(async move |_, cx| {
            cx.background_executor().timer(RESIZE_DEBOUNCE).await;
            session.update(cx, |session, cx| apply_resize(session, dims, cx));
        }));
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx);
        let label = self.session.read(cx).label().clone();
        let hide_label = SharedString::from(navo_l10n::tr("terminal-drawer-hide"));
        let on_hide = self.on_hide.clone();
        div()
            .id("terminal-drawer-header")
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(tokens::HEADER_HEIGHT))
            .px(t.spacing.sm)
            .bg(t.colors.background)
            .border_b_1()
            .border_color(t.colors.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.xs)
                    .min_w_0()
                    .child(icon(
                        IconName::SquareTerminal,
                        px(HEADER_ICON_SIZE),
                        t.colors.accent_text,
                    ))
                    .child(
                        div()
                            .text_size(t.typography.small)
                            .text_color(t.colors.text)
                            .truncate()
                            .child(label),
                    ),
            )
            .child(
                IconButton::new("terminal-drawer-hide", IconName::ChevronDown, hide_label)
                    .on_click(move |_, window, cx| {
                        if let Some(hide) = &on_hide {
                            hide(window, cx);
                        }
                    }),
            )
    }

    fn grid_body(&self, mono_font: SharedString, cx: &App) -> impl IntoElement {
        let snapshot = self.session.read(cx).snapshot();
        let style = GridStyle::default();
        let measured = Rc::clone(&self.measured_dims);
        let font_family = style.font_family.clone().unwrap_or(mono_font);
        let font_size = style.font_size;
        let line_height = style.line_height;
        let pad = style.padding;
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(TerminalGrid::new(snapshot, self.cache.clone()).style(style))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let metrics = measure_cell(window, &font_family, font_size, line_height);
                        let inner_w = (bounds.size.width - pad * 2.0).max(px(0.0));
                        let inner_h = (bounds.size.height - pad * 2.0).max(px(0.0));
                        let dims = grid_dimensions(inner_w, inner_h, metrics.width, metrics.height);
                        if dims.0 > 0 && dims.1 > 0 {
                            measured.set(dims);
                        }
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

impl Render for TerminalDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_measured_size(cx);
        let t = tokens::current(cx).clone();
        let border = if self.focus.is_focused(window) {
            t.colors.accent
        } else {
            t.colors.line
        };
        div()
            .id("terminal-drawer")
            .test_support()
            .aria_label(navo_l10n::tr("terminal-drawer-label"))
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(t.terminal.background)
            .border_1()
            .border_color(border)
            .rounded(t.radii.card)
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(self.header(cx))
            .child(self.grid_body(t.typography.mono_font.clone(), cx))
    }
}

fn apply_resize(
    session: &mut TerminalSession,
    dims: (u16, u16),
    cx: &mut Context<TerminalSession>,
) {
    if session.resize(dims.0, dims.1).is_ok() {
        cx.notify();
    }
}

fn is_paste_shortcut(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key.eq_ignore_ascii_case("v") && m.control && m.shift && !m.alt && !m.platform
}

/// Notice shown when the user presses `Ctrl+\`` while `[modules] terminal` is off.
#[derive(IntoElement)]
pub struct TerminalOffNotice {
    on_turn_on: Callback,
    on_external: Callback,
    on_dismiss: Callback,
}

impl TerminalOffNotice {
    /// Builds the module-off notice with callbacks for Turn on…, External terminal, and Dismiss.
    pub fn new(
        on_turn_on: impl Fn(&mut Window, &mut App) + 'static,
        on_external: impl Fn(&mut Window, &mut App) + 'static,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            on_turn_on: Rc::new(on_turn_on),
            on_external: Rc::new(on_external),
            on_dismiss: Rc::new(on_dismiss),
        }
    }
}

fn notice_actions(t: &tokens::Theme, on_turn_on: Callback, on_external: Callback) -> Div {
    let turn_on_label = SharedString::from(navo_l10n::tr("terminal-off-turn-on"));
    let external_label = SharedString::from(navo_l10n::tr("cmd-files-open-terminal"));
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(t.spacing.sm)
        .pt(t.spacing.xs)
        .child(
            Button::new("terminal-off-turn-on", turn_on_label)
                .variant(ButtonVariant::Primary)
                .on_click(move |_, window, cx| on_turn_on(window, cx)),
        )
        .child(
            Button::new("terminal-off-external", external_label)
                .variant(ButtonVariant::Secondary)
                .on_click(move |_, window, cx| on_external(window, cx)),
        )
}

impl RenderOnce for TerminalOffNotice {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let title = SharedString::from(navo_l10n::tr("terminal-off-title"));
        let body = SharedString::from(navo_l10n::tr("terminal-off-body"));
        let dismiss_label = SharedString::from(navo_l10n::tr("terminal-off-dismiss"));
        let on_dismiss = self.on_dismiss;

        div()
            .id("terminal-off-notice")
            .role(Role::Status)
            .aria_label(title.clone())
            .test_support()
            .flex()
            .items_start()
            .gap(t.spacing.md)
            .max_w(px(NOTICE_MAX_WIDTH))
            .p(t.spacing.md)
            .bg(t.surfaces.sheet)
            .border_1()
            .border_color(t.colors.line)
            .rounded(t.radii.sheet)
            .shadow(t.shadows.menu.clone())
            .child(div().pt(t.spacing.xxs).flex_none().child(icon(
                IconName::SquareTerminal,
                px(NOTICE_ICON_SIZE),
                t.colors.accent_text,
            )))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(t.spacing.xs)
                    .child(
                        div()
                            .text_size(t.typography.body)
                            .text_color(t.colors.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(t.typography.small)
                            .text_color(t.colors.muted)
                            .child(body),
                    )
                    .child(notice_actions(t, self.on_turn_on, self.on_external)),
            )
            .child(
                IconButton::new("terminal-off-dismiss", IconName::Close, dismiss_label)
                    .on_click(move |_, window, cx| on_dismiss(window, cx)),
            )
    }
}

/// Builds the title-bar Terminal toggle button shown while `[modules] terminal` is on.
pub fn title_bar_terminal_button(
    is_open: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let t = tokens::current(cx);
    let label = SharedString::from(navo_l10n::tr("terminal-title-button"));
    let tip = SharedString::from(navo_l10n::tr("terminal-title-button-tip"));
    let active_bg = t.colors.selection;
    let active_fg = t.colors.accent_text;
    let rest_fg = t.colors.text;
    let hover_bg = t.colors.hover;

    div()
        .id("title-terminal-button")
        .role(Role::Button)
        .aria_label(label.clone())
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xs)
        .px(t.spacing.sm)
        .h(t.spacing.button_size)
        .rounded(t.radii.md)
        .text_size(t.typography.small)
        .text_color(if is_open { active_fg } else { rest_fg })
        .when(is_open, |el| el.bg(active_bg))
        .cursor_pointer()
        .hover(move |el| el.bg(hover_bg))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(icon(
            IconName::SquareTerminal,
            px(HEADER_ICON_SIZE),
            if is_open { active_fg } else { t.colors.muted },
        ))
        .child(label)
}
