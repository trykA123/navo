//! The floating Settings window view: Navo's title bar and Navo's settings content.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{Subscription, WeakEntity, WindowHandle};
use navo_l10n::t;

use super::super::NavoWindow;
use super::super::settings_sheet::SettingsSnapshot;
use super::{SettingsRoute, update_navo};
use crate::prelude::*;
use crate::settings::{self, ToggleSettings};
use crate::title_bar::NavoTitleBar;
use crate::tokens;

/// Root content view for the floating Settings window; all settings state stays on `NavoWindow`
/// and is mirrored into `snapshot` outside `render` so drawing this window never reassigns
/// `NavoWindow`'s tracked GPUI window.
pub(in crate::window_view) struct SettingsWindow {
    navo: WeakEntity<NavoWindow>,
    main: WindowHandle<NavoWindow>,
    snapshot: Option<SettingsSnapshot>,
    _watches: Vec<Subscription>,
}

impl SettingsWindow {
    /// Builds the Settings view, mirroring `NavoWindow` snapshots on notifications and theme updates.
    pub(super) fn new(
        navo: WeakEntity<NavoWindow>,
        main: WindowHandle<NavoWindow>,
        snapshot: Option<SettingsSnapshot>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut watches = Vec::new();
        if let Some(entity) = navo.upgrade() {
            watches.push(cx.observe(&entity, |view, navo, cx| {
                view.snapshot = Some(navo.read(cx).settings_snapshot());
                cx.notify();
            }));
        }
        watches.push(cx.observe_global::<tokens::Theme>(|_, cx| cx.notify()));
        Self {
            navo,
            main,
            snapshot,
            _watches: watches,
        }
    }

    /// Replaces the mirrored Settings snapshot and schedules a repaint.
    pub(super) fn set_snapshot(
        &mut self,
        snapshot: Option<SettingsSnapshot>,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot;
        cx.notify();
    }
}

/// Defers closing the Settings window until its update lease ends.
fn defer_close(main: WindowHandle<NavoWindow>, settings: AnyWindowHandle, cx: &mut App) {
    cx.defer(move |cx| request_close(main, settings, cx));
}

/// Routes a close request to Navo so the host closes the window on Navo's side.
fn request_close(main: WindowHandle<NavoWindow>, settings: AnyWindowHandle, cx: &mut App) {
    if update_navo(&main, cx, |navo, window, cx| {
        navo.close_settings_window(window, cx);
    })
    .is_none()
    {
        tracing::warn!("a settings close arrived after Navo's window closed");
        let _ = cx.update_window(settings, |_, window, _| window.remove_window());
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
            .on_action(cx.listener(|view, _: &ToggleSettings, window, cx| {
                defer_close(view.main, window.window_handle(), cx);
            }));
        let Some(snapshot) = &self.snapshot else {
            return root;
        };
        let route = SettingsRoute {
            navo: self.navo.clone(),
            main: Some(main),
        };
        let on_event = NavoWindow::settings_listener(main, |navo, event, window, cx| {
            navo.settings_window_event(event, window, cx);
        });
        let body = snapshot.build_body(&route, on_event, &t);
        root.child(
            NavoTitleBar::new(t!("settings-title"))
                .menu_arrow(false)
                .on_close(move |window, cx| defer_close(main, window.window_handle(), cx)),
        )
        .child(settings::content(body))
    }
}
