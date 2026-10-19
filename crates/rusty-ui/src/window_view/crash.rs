use std::sync::Arc;

use gpui_kit::FocusHandle;
use rusty_core::Location;

use super::FerriteWindow;
use super::keys::{CRASH, DismissCrashNotice, FocusCrashNotice, SaveCrashReport};
use super::path_edit::last_name;
use crate::commands::active;
use crate::crash_notice::{CrashKeys, CrashNotice};
use crate::prelude::*;
use crate::status_bar::StatusNotice;
use crate::tokens;

/// Both calls clear the pending crash, so the notice shows once per crash.
pub trait CrashReportService: Send + Sync {
    fn save(&self) -> Result<Location, String>;
    fn dismiss(&self);
}

pub(super) struct CrashState {
    service: Option<Arc<dyn CrashReportService>>,
    pub(super) error: Option<SharedString>,
    is_saving: bool,
    focus: FocusHandle,
}

impl CrashState {
    pub(super) fn new(service: Option<Arc<dyn CrashReportService>>, cx: &App) -> Self {
        Self {
            service,
            error: None,
            is_saving: false,
            focus: cx.focus_handle(),
        }
    }
}

fn saved_text(zip: &Location) -> Option<(Location, String, String)> {
    let name = last_name(zip)?;
    let folder = zip.parent()?;
    let place = last_name(&folder).unwrap_or_else(|| folder.path().display().to_string());
    let text = format!("Saved {name} in {place}");
    Some((folder, name, text))
}

impl FerriteWindow {
    pub(super) fn crash_notice_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.crash.service.as_ref()?;
        let t = tokens::current(cx).clone();
        let (save, dismiss) = (cx.weak_entity(), cx.weak_entity());
        let notice = CrashNotice::new(CrashKeys::of(active(cx)))
            .error(self.crash.error.clone())
            .focused(self.crash.focus.is_focused(window))
            .saving(self.crash.is_saving)
            .on_save(move |window, cx| {
                let _ = save.update(cx, |view, cx| view.save_crash_report(window, cx));
            })
            .on_dismiss(move |window, cx| {
                let _ = dismiss.update(cx, |view, cx| view.dismiss_crash_notice(window, cx));
            });
        let element = div()
            .id("crash-notice")
            .key_context(CRASH)
            .track_focus(&self.crash.focus)
            .on_action(cx.listener(|view, _: &SaveCrashReport, window, cx| {
                view.save_crash_report(window, cx);
            }))
            .on_action(cx.listener(|view, _: &DismissCrashNotice, window, cx| {
                view.dismiss_crash_notice(window, cx);
            }))
            .flex_none()
            .mx(t.spacing.xs * 2.)
            .my(t.spacing.xs)
            .child(notice);
        Some(element.into_any_element())
    }

    pub(super) fn crash_keys(&self, root: Div, cx: &mut Context<Self>) -> Div {
        if self.crash.service.is_none() {
            return root;
        }
        root.on_action(cx.listener(|view, _: &FocusCrashNotice, window, cx| {
            window.focus(&view.crash.focus, cx);
            cx.notify();
        }))
    }

    fn save_crash_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(service) = self.crash.service.clone() else {
            return;
        };
        if self.crash.is_saving {
            return;
        }
        self.crash.is_saving = true;
        cx.notify();
        let saving = cx.background_spawn(async move { service.save() });
        cx.spawn_in(window, async move |this, cx| {
            let saved = saving.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.crash_report_saved(saved, window, cx);
            });
        })
        .detach();
    }

    fn crash_report_saved(
        &mut self,
        saved: Result<Location, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.crash.is_saving = false;
        match saved {
            Ok(zip) => {
                self.crash.service = None;
                self.crash.error = None;
                if let Some((folder, name, text)) = saved_text(&zip) {
                    self.browser.open_tab_at(folder, Some(name));
                    self.show_notice(StatusNotice {
                        text: text.into(),
                        is_error: false,
                    });
                }
                self.focus_core_pane(window, cx);
            }
            Err(error) => self.crash.error = Some(error.into()),
        }
        cx.notify();
    }

    fn dismiss_crash_notice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.crash.is_saving {
            return;
        }
        let Some(service) = self.crash.service.take() else {
            return;
        };
        service.dismiss();
        self.crash.error = None;
        self.focus_core_pane(window, cx);
        cx.notify();
    }
}
