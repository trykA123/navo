use crate::prelude::*;
use crate::settings::Field;
use crate::window_view::FerriteWindow;

const NO_FILE: &str = "There is no settings file in this session.";

fn missing_note(key: &str) -> SharedString {
    format!("{key} is not in navo.toml; the default applies.").into()
}

impl FerriteWindow {
    pub(super) fn open_settings_file(
        &mut self,
        field: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saver.is_busy() {
            self.settings.waiting_locate = Some(field);
            return;
        }
        let Some(file) = self.settings.file.clone() else {
            self.settings.notice = Some(NO_FILE.into());
            return;
        };
        let line_of = self.settings.line_of.clone();
        let key = field.spec().key;
        let task = cx.background_executor().spawn(async move { line_of(key) });
        cx.spawn_in(window, async move |this, cx| {
            let found = task.await;
            let note = found
                .filter(|found| found.is_missing)
                .map(|_| missing_note(key));
            let _ = this.update_in(cx, |view, window, cx| {
                view.view_location(file, window, cx);
                view.open_viewer_at_line(found.map(|found| found.line), note);
            });
        })
        .detach();
    }

    pub(in crate::window_view) fn locate_after_save(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.saver.is_busy() || !self.settings.is_open() {
            return;
        }
        if let Some(field) = self.settings.waiting_locate.take() {
            self.open_settings_file(field, window, cx);
        }
    }
}
