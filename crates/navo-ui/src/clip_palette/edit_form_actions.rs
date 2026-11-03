//! The palette's commands for the saved item form: open it new or for an item, save, ask
//! before cancelling, show the password, and note edits.

use gpui_kit::{Context, Focusable, Window};
use navo_l10n::tr;

use super::edit_form::{Form, FormKind, Initial, same_scope};
use super::manage_kinds::Kind;
use super::notes::{folder_key, folder_name};
use super::view::ClipPalette;
use super::{ClipId, ClipMode, Scope};

impl ClipPalette {
    /// The scopes the form offers: everywhere, the active folder and the item's own scope.
    fn scopes_for(&self, own: &Scope) -> (Vec<Scope>, usize) {
        let active = self.state.folder().cloned().map(Scope::Folder);
        let mut scopes = vec![Scope::Everywhere];
        for scope in active.into_iter().chain([own.clone()]) {
            if !scopes.iter().any(|known| same_scope(known, &scope)) {
                scopes.push(scope);
            }
        }
        let at = scopes.iter().position(|scope| same_scope(scope, own));
        (scopes, at.unwrap_or(0))
    }

    /// The label of a scope chip: the active folder reads "This folder".
    pub(super) fn scope_label(&self, scope: &Scope) -> String {
        match scope {
            Scope::Everywhere => tr("clip-form-scope-everywhere"),
            Scope::Folder(at) => match self.state.folder() {
                Some(active) if folder_key(active) == folder_key(at) => {
                    tr("clip-form-scope-folder")
                }
                _ => folder_name(at),
            },
        }
    }

    fn start_form(
        &mut self,
        (target, initial): (Option<ClipId>, Initial),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.overlay.menu = None;
        self.overlay.revealed = None;
        self.overlay.status = None;
        self.repoint = None;
        let scopes = self.scopes_for(&initial.scope);
        let form = Form::new(target, initial, scopes, (window, cx));
        window.focus(&form.title.focus_handle(cx), cx);
        self.form = Some(form);
        cx.notify();
    }

    /// Ctrl+N: an empty form; in Secrets it starts as a secret. Nothing when the form is open
    /// or the hooks offer neither saved items nor secrets.
    pub fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modes = self.state.modes();
        let can_save = modes.contains(&ClipMode::Saved) || modes.contains(&ClipMode::Secrets);
        if self.form.is_some() || !can_save {
            return;
        }
        let in_secrets = match &self.manage {
            Some(manage) => manage.kind() == Kind::Secrets,
            None => self.state.mode() == ClipMode::Secrets,
        };
        let kind = if in_secrets {
            FormKind::Secret
        } else {
            FormKind::Snippet
        };
        self.start_form((None, Initial::blank(kind)), window, cx);
    }

    /// F2 on a saved row: the form with its values. A secret's password stays empty.
    pub(super) fn open_edit(&mut self, id: ClipId, window: &mut Window, cx: &mut Context<Self>) {
        match self.hooks.saved_for_edit(id) {
            Ok(edit) => self.start_form((Some(id), Initial::of(edit)), window, cx),
            Err(message) => self.say(message, true, cx),
        }
    }

    /// Drops the form with its password and puts the focus back on the search field.
    pub(super) fn close_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.take().is_some() {
            window.focus(&self.field.focus_handle(cx), cx);
            cx.notify();
        }
    }

    /// Ctrl+Enter and Ctrl+S: saves, or shows what is missing under its field.
    pub(super) fn save_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let hooks = self.hooks.clone();
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match form.submit(hooks.as_ref(), cx) {
            Ok(()) => {
                self.close_form(window, cx);
                self.say(tr("clip-status-saved-item"), false, cx);
            }
            Err(problem) => {
                if let Some(field) = problem.field() {
                    window.focus(&form.handle(field, cx), cx);
                }
                form.problem = Some(problem);
                cx.notify();
            }
        }
    }

    /// Esc: asks first when something changed, and closes when nothing did or after a yes.
    pub(super) fn form_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.is_asking {
            form.is_asking = false;
            cx.notify();
        } else if form.is_dirty(cx) {
            form.is_asking = true;
            cx.notify();
        } else {
            self.close_form(window, cx);
        }
    }

    /// Alt+R: shows or hides the typed password.
    pub(super) fn form_reveal(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut().filter(|f| f.kind == FormKind::Secret) {
            form.password.toggle_shown();
            cx.notify();
        }
    }

    /// A field changed: a message under it is out of date, and so is the question.
    pub(super) fn form_edited(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.problem = None;
            form.is_asking = false;
            cx.notify();
        }
    }
}
