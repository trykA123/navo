//! Terminal overlay states: inline exit bar, missing-shell empty state, module-off notice, and
//! title-bar Terminal toggle button.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{RenderOnce, Role};

use super::session::MissingShellInfo;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, IconButton, IconName, Tooltip, icon};
use crate::tokens;

const HEADER_ICON_SIZE: f32 = 14.0;
const NOTICE_ICON_SIZE: f32 = 18.0;
const EMPTY_ICON_SIZE: f32 = 22.0;
const NOTICE_MAX_WIDTH: f32 = 440.0;

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

/// Renders the inline exit bar shown below an exited session while its scrollback stays visible.
pub fn render_exit_bar(
    exit_code: Option<u32>,
    on_restart: Callback,
    on_close: Callback,
    cx: &App,
) -> AnyElement {
    let t = tokens::current(cx);
    let message: SharedString = match exit_code {
        Some(code) => navo_l10n::t!("terminal-exited-with-code", code = code).into(),
        None => navo_l10n::tr("terminal-exited").into(),
    };
    let (status_icon, status_color) = match exit_code {
        Some(0) | None => (IconName::Info, t.colors.muted),
        Some(_) => (IconName::CircleX, t.colors.mark),
    };
    let restart_label = SharedString::from(navo_l10n::tr("terminal-exit-restart"));
    let restart_tip = SharedString::from(navo_l10n::tr("terminal-exit-restart-tip"));
    let close_label = SharedString::from(navo_l10n::tr("terminal-exit-close"));

    div()
        .id("terminal-exit-bar")
        .role(Role::Status)
        .aria_label(message.clone())
        .test_support()
        .flex()
        .flex_none()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(t.spacing.sm)
        .px(t.spacing.md)
        .py(t.spacing.xs)
        .rounded_b(t.radii.card)
        .bg(t.colors.background)
        .border_t_1()
        .border_color(t.colors.line)
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.xs)
                .min_w_0()
                .child(icon(status_icon, px(HEADER_ICON_SIZE), status_color))
                .child(
                    div()
                        .text_size(t.typography.small)
                        .text_color(t.colors.text)
                        .truncate()
                        .child(message),
                ),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(t.spacing.xs)
                .child(
                    Button::new("terminal-exit-restart", restart_label)
                        .variant(ButtonVariant::Primary)
                        .tooltip(restart_tip)
                        .on_click(move |_, window, cx| on_restart(window, cx)),
                )
                .child(
                    Button::new("terminal-exit-close", close_label)
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| on_close(window, cx)),
                ),
        )
        .into_any_element()
}

/// Renders the empty state shown when the configured shell is missing or failed to spawn.
pub fn render_missing_shell(
    info: &MissingShellInfo,
    has_shells: bool,
    on_start_fallback: Callback,
    on_open_picker: Callback,
    cx: &App,
) -> AnyElement {
    let t = tokens::current(cx);
    let title: SharedString = if info.missing.is_empty() {
        navo_l10n::tr("terminal-missing-none-title").into()
    } else {
        navo_l10n::t!("terminal-missing-title", shell = info.missing.as_ref()).into()
    };
    let body: SharedString = match &info.fallback {
        Some(fallback) => {
            navo_l10n::t!("terminal-missing-body", shell = fallback.label.as_str()).into()
        }
        None => navo_l10n::tr("terminal-missing-none-body").into(),
    };
    let fallback_btn = info.fallback.as_ref().map(|fallback| {
        let label: SharedString =
            navo_l10n::t!("terminal-missing-start", shell = fallback.label.as_str()).into();
        let on_start = Rc::clone(&on_start_fallback);
        Button::new("terminal-missing-start-fallback", label)
            .variant(ButtonVariant::Primary)
            .on_click(move |_, window, cx| on_start(window, cx))
    });
    let choose_btn = has_shells.then(|| {
        let label = SharedString::from(navo_l10n::tr("terminal-missing-choose"));
        Button::new("terminal-missing-choose-shell", label)
            .variant(ButtonVariant::Secondary)
            .on_click(move |_, window, cx| on_open_picker(window, cx))
    });

    div()
        .id("terminal-missing-shell")
        .role(Role::Status)
        .aria_label(title.clone())
        .test_support()
        .flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(t.spacing.sm)
        .p(t.spacing.lg)
        .child(icon(
            IconName::TriangleAlert,
            px(EMPTY_ICON_SIZE),
            t.colors.warn,
        ))
        .child(
            div()
                .text_size(t.typography.body)
                .text_color(t.colors.text)
                .child(title),
        )
        .child(
            div()
                .max_w(px(NOTICE_MAX_WIDTH))
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(body),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_center()
                .gap(t.spacing.sm)
                .pt(t.spacing.xs)
                .children(fallback_btn)
                .children(choose_btn),
        )
        .into_any_element()
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
