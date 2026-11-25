use navo_core::Location;

use crate::prelude::*;
use crate::settings::Field;
use crate::window_view::NavoWindow;
use crate::window_view::path_edit::last_name;

const NO_FILE: &str = "There is no settings file in this session.";

fn missing_note(key: &str) -> SharedString {
    format!("{key} is not in navo.toml; the default applies.").into()
}

impl NavoWindow {
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

    pub(super) fn open_data_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self.settings.data_folder.clone().or_else(|| {
            self.settings
                .diagnostics
                .first()
                .and_then(|row| Location::new(row.path.as_ref()).ok())
                .and_then(|loc| loc.parent())
        });
        let Some(folder) = folder else {
            self.settings.notice = Some(navo_l10n::tr("settings-storage-no-folder").into());
            return;
        };
        self.reveal_storage_folder(folder, None, window, cx);
    }

    pub(super) fn open_diagnostic_path(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(location) = Location::new(path) else {
            self.settings.notice = Some(navo_l10n::tr("settings-storage-no-folder").into());
            return;
        };
        match location.parent() {
            Some(parent) => self.reveal_storage_folder(parent, last_name(&location), window, cx),
            None => self.reveal_storage_folder(location, None, window, cx),
        }
    }

    fn reveal_storage_folder(
        &mut self,
        folder: Location,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_settings(window, cx);
        self.browser.open_tab_at(folder, name);
        self.focus_core_pane(window, cx);
    }
}
