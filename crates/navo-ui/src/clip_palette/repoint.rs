//! F2 on a note whose folder is gone offers to point the note at the active folder (Q26). The
//! question sits in the footer; Enter moves the note, Esc leaves it where it is.

use gpui_kit::{
    Context, Focusable, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div,
};
use navo_core::Location;
use navo_l10n::{t, tr};

use super::chrome::{footer_bar, footer_button};
use super::notes::folder_name;
use super::rows::RowElement;
use super::view::ClipPalette;
use super::{ClipRow, Scope};
use crate::tokens::Theme;

/// A note waiting to move: the scope it has now and the folder it would point at.
pub(super) struct Repoint {
    from: Scope,
    to: Location,
}

impl ClipPalette {
    /// Asks before moving the note of `row`, if its folder is gone and the active folder has
    /// no note of its own. Says why when it cannot be offered.
    pub(super) fn offer_repoint(
        &mut self,
        row: &ClipRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_gone =
            matches!(&row.scope, Scope::Folder(at) if self.missing_folders().contains(at));
        if !is_gone {
            return;
        }
        let Some(to) = self.state.folder().cloned() else {
            return self.say(tr("clip-status-no-folder"), true, cx);
        };
        if self.hooks.note_for(&to).is_some() {
            return self.say(tr("clip-status-note-exists"), true, cx);
        }
        self.repoint = Some(Repoint {
            from: row.scope.clone(),
            to,
        });
        window.focus(&self.field.focus_handle(cx), cx);
        cx.notify();
    }

    /// Enter on the question: moves the note and says where it went.
    pub(super) fn confirm_repoint(&mut self, cx: &mut Context<Self>) {
        let Some(Repoint { from, to }) = self.repoint.take() else {
            return;
        };
        self.flush_note(cx);
        let name = folder_name(&to);
        match self.hooks.rename_note(from, Scope::Folder(to)) {
            Ok(()) => self.say(t!("clip-status-repointed", name = name), false, cx),
            Err(message) => self.say(message, true, cx),
        }
    }

    /// Esc on the question. False when none was showing.
    pub(super) fn cancel_repoint(&mut self, cx: &mut Context<Self>) -> bool {
        let was_asking = self.repoint.take().is_some();
        if was_asking {
            cx.notify();
        }
        was_asking
    }

    /// The footer that carries the question, in place of the usual one.
    pub(super) fn repoint_footer(&self, t: &Theme, cx: &mut Context<Self>) -> Option<RowElement> {
        let ask = self.repoint.as_ref()?;
        let question = t!("clip-repoint-ask", name = folder_name(&ask.to));
        let yes = footer_button(t, "clip-repoint-yes", tr("clip-repoint-yes"), "enter")
            .on_click(cx.listener(|this, _, _, cx| this.confirm_repoint(cx)));
        let no = footer_button(t, "clip-repoint-no", tr("clip-repoint-no"), "escape").on_click(
            cx.listener(|this, _, _, cx| {
                this.cancel_repoint(cx);
            }),
        );
        let status = div().text_color(t.colors.text).child(question);
        Some(footer_bar(
            t,
            status,
            vec![yes.into_any_element(), no.into_any_element()],
        ))
    }
}
