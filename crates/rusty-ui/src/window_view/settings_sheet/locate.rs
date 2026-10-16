use crate::prelude::*;
use crate::settings::Field;
use crate::window_view::FerriteWindow;

const NO_FILE: &str = "There is no settings file in this session.";

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
            let line = task.await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.view_location(file, window, cx);
                view.open_viewer_at_line(line);
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
