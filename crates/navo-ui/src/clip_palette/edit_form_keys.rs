//! Keys in the form. While it is open every palette key that moves, enters, copies or deletes
//! belongs to it: Tab and the arrows walk the fields, and the text fields get the rest.

use gpui_kit::{Context, KeyDownEvent, Window};

use super::edit_form::{Field, Form, FormKind};
use super::note_editor::Pass;
use super::view::ClipPalette;

impl Form {
    /// Moves the focus to the next or previous field, wrapping.
    fn go(&self, delta: isize, window: &mut Window, cx: &mut gpui_kit::App) {
        let order = self.order();
        let at = self
            .focused(window, cx)
            .and_then(|field| order.iter().position(|known| *known == field))
            .unwrap_or(0);
        let last = order.len() - 1;
        let to = match at.checked_add_signed(delta) {
            Some(to) if to <= last => to,
            Some(_) => 0,
            None => last,
        };
        window.focus(&self.handle(order[to], cx), cx);
    }

    /// Left and Right on the kind or scope chips.
    fn step_choice(&mut self, delta: isize, window: &Window, cx: &gpui_kit::App) -> bool {
        match self.focused(window, cx) {
            Some(Field::Kind) => {
                let at = FormKind::ALL.iter().position(|kind| *kind == self.kind);
                let to = at.unwrap_or(0).saturating_add_signed(delta);
                self.set_kind(FormKind::ALL[to.min(FormKind::ALL.len() - 1)]);
                true
            }
            Some(Field::Scope) => {
                let to = self.scope.saturating_add_signed(delta);
                self.scope = to.min(self.scopes.len() - 1);
                true
            }
            _ => false,
        }
    }
}

impl ClipPalette {
    /// A palette key while the form is open. True whenever it is: the palette never acts on it.
    pub(super) fn form_key(
        &mut self,
        key: Pass,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.form.as_ref() else {
            return false;
        };
        if form.is_asking {
            if matches!(key, Pass::Enter) {
                self.close_form(window, cx);
            }
            return true;
        }
        let at = form.focused(window, cx);
        let is_text = matches!(at, Some(Field::Title | Field::User | Field::Body));
        let is_body = at == Some(Field::Body);
        let is_last = !is_body && form.order().last().copied() == at;
        match key {
            Pass::Indent => form.go(1, window, cx),
            Pass::Outdent => form.go(-1, window, cx),
            Pass::Enter if is_last => self.save_form(window, cx),
            Pass::Down | Pass::Enter if !is_body => form.go(1, window, cx),
            Pass::Up if !is_body => form.go(-1, window, cx),
            Pass::Up | Pass::Down | Pass::Enter => window.dispatch_action(key.action(), cx),
            Pass::Copy | Pass::Delete | Pass::Paste if is_text => {
                window.dispatch_action(key.action(), cx);
            }
            Pass::Paste if at == Some(Field::Password) => self.paste_password(cx),
            Pass::Copy | Pass::Delete | Pass::Paste => {}
        }
        cx.notify();
        true
    }

    fn paste_password(&mut self, cx: &mut Context<Self>) {
        let (Some(form), Some(item)) = (self.form.as_mut(), cx.read_from_clipboard()) else {
            return;
        };
        form.password.paste_item(item);
        self.form_edited(cx);
    }

    /// Left and Right: change the kind or the scope while one of those has the focus.
    pub(super) fn step_choice(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.step_choice(delta, window, cx) {
            self.form_edited(cx);
        }
    }

    /// A typed key in the password field.
    pub(super) fn password_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.password.key(event) {
            cx.stop_propagation();
            self.form_edited(cx);
        }
    }
}
