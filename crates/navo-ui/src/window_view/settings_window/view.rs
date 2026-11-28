//! The floating Settings window view: Navo's title bar and Navo's settings content.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Subscription, WeakEntity, WindowHandle};
use navo_l10n::t;

use super::super::NavoWindow;
use super::update_navo;
use crate::prelude::*;
use crate::settings::{self, ToggleSettings};
use crate::title_bar::NavoTitleBar;
use crate::tokens;

/// Root content view for the floating Settings window; all settings state stays on `NavoWindow`.
pub(in crate::window_view) struct SettingsWindow {
    navo: WeakEntity<NavoWindow>,
    main: WindowHandle<NavoWindow>,
    _watches: Vec<Subscription>,
}

impl SettingsWindow {
    /// Builds the Settings view, watching Navo and the theme global for redraws.
    pub(super) fn new(
        navo: WeakEntity<NavoWindow>,
        main: WindowHandle<NavoWindow>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut watches = Vec::new();
        if let Some(entity) = navo.upgrade() {
            watches.push(cx.observe(&entity, |_, _, cx| cx.notify()));
        }
        watches.push(cx.observe_global::<tokens::Theme>(|_, cx| cx.notify()));
        Self {
            navo,
            main,
            _watches: watches,
        }
    }
}

/// Routes a close request to Navo so the host closes the window on Navo's side.
pub(super) fn request_close(main: WindowHandle<NavoWindow>, cx: &mut App) {
    if update_navo(&main, cx, |navo, window, cx| {
        navo.close_settings_window(window, cx);
    })
    .is_none()
    {
        tracing::warn!("a settings close arrived after Navo's window closed");
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let backdrop = if t.is_contrast {
            gpui_kit::WindowBackgroundAppearance::Opaque
        } else {
            crate::title_bar::system_backdrop().window
        };
        window.set_background_appearance(backdrop);
        let main = self.main;
        let root = div()
            .id("settings-window")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .text_color(t.colors.text)
            .font_family(t.typography.ui_font.clone())
            .text_size(t.typography.body)
            .when(t.is_contrast, |root| root.bg(t.colors.background))
            .on_action(cx.listener(|view, _: &ToggleSettings, _, cx| {
                request_close(view.main, cx);
            }));
        let Some(navo) = self.navo.upgrade() else {
            return root;
        };
        let body = navo.update(cx, |navo, cx| {
            let on_event = NavoWindow::settings_listener(main, |navo, event, window, cx| {
                navo.settings_window_event(event, window, cx);
            });
            navo.settings_body(on_event, window, cx)
        });
        root.child(
            NavoTitleBar::new(t!("settings-title"))
                .menu_arrow(false)
                .on_close(move |_, cx| request_close(main, cx)),
        )
        .child(settings::content(body))
    }
}
