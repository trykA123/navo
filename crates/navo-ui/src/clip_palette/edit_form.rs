//! The form for a new or edited saved item. It replaces the palette body while it is open. The
//! password lives in one wiped buffer that is dropped with the form: on save, cancel, hide and
//! close. Titles, user names and passwords stay out of logs and `Debug`.

use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, Subscription, Window,
};
use navo_l10n::{t, tr};

use super::note_editor::plain_editor;
use super::notes::folder_key;
use super::password::Password;
use super::view::ClipPalette;
use super::{ClipId, ClipKind, ClipboardHooks, SavedDraft, SavedEdit, Scope, SecretText};
use crate::primitives::{TextFieldState, text_field_state};

pub(super) const TITLE_MAX: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FormKind {
    Snippet,
    Path,
    Secret,
}

impl FormKind {
    pub const ALL: [Self; 3] = [Self::Snippet, Self::Path, Self::Secret];

    fn of(kind: ClipKind) -> Self {
        match kind {
            ClipKind::Secret => Self::Secret,
            ClipKind::Path | ClipKind::Files => Self::Path,
            ClipKind::Text | ClipKind::Link | ClipKind::Image | ClipKind::Note => Self::Snippet,
        }
    }

    fn clip_kind(self) -> ClipKind {
        match self {
            Self::Snippet => ClipKind::Text,
            Self::Path => ClipKind::Path,
            Self::Secret => ClipKind::Secret,
        }
    }

    pub fn label(self) -> String {
        tr(match self {
            Self::Snippet => "clip-form-kind-snippet",
            Self::Path => "clip-form-kind-path",
            Self::Secret => "clip-form-kind-secret",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Kind,
    Title,
    Body,
    User,
    Password,
    Scope,
}

/// Why a save did not happen. `Refused` carries the hooks' own message.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Problem {
    NoTitle,
    TitleTooLong,
    NoPassword,
    NoBody,
    Refused(String),
}

impl Problem {
    /// The field the message belongs under; none for a refusal by the hooks.
    pub fn field(&self) -> Option<Field> {
        match self {
            Self::NoTitle | Self::TitleTooLong => Some(Field::Title),
            Self::NoPassword => Some(Field::Password),
            Self::NoBody => Some(Field::Body),
            Self::Refused(_) => None,
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::NoTitle => tr("clip-form-no-title"),
            Self::TitleTooLong => t!("clip-form-title-long", max = TITLE_MAX),
            Self::NoPassword => tr("clip-form-no-password"),
            Self::NoBody => tr("clip-form-no-body"),
            Self::Refused(message) => message.clone(),
        }
    }
}

/// What the form starts with.
pub(super) struct Initial {
    pub kind: FormKind,
    pub title: String,
    pub body: String,
    pub user: String,
    pub scope: Scope,
}

impl Initial {
    pub fn blank(kind: FormKind) -> Self {
        Self {
            kind,
            title: String::new(),
            body: String::new(),
            user: String::new(),
            scope: Scope::Everywhere,
        }
    }

    pub fn of(edit: SavedEdit) -> Self {
        Self {
            kind: FormKind::of(edit.kind),
            title: edit.title,
            body: edit.body.expose().to_owned(),
            user: edit.user.unwrap_or_default(),
            scope: edit.scope,
        }
    }
}

pub(super) fn same_scope(a: &Scope, b: &Scope) -> bool {
    match (a, b) {
        (Scope::Everywhere, Scope::Everywhere) => true,
        (Scope::Folder(a), Scope::Folder(b)) => folder_key(a) == folder_key(b),
        _ => false,
    }
}

/// The values the form was opened with, to tell whether anything changed.
struct Start {
    kind: FormKind,
    title: String,
    body: String,
    user: String,
    scope: usize,
}

pub(super) struct Form {
    /// The item being edited; none for a new one.
    target: Option<ClipId>,
    was_secret: bool,
    pub kind: FormKind,
    pub scopes: Vec<Scope>,
    pub scope: usize,
    pub title: Entity<TextFieldState>,
    pub user: Entity<TextFieldState>,
    pub body: Entity<EditorState>,
    pub password: Password,
    pub kind_focus: FocusHandle,
    pub scope_focus: FocusHandle,
    start: Start,
    pub problem: Option<Problem>,
    /// "Discard changes?" is showing.
    pub is_asking: bool,
    _changes: [Subscription; 3],
}

fn on_edit<T: 'static>(
    palette: &mut ClipPalette,
    _: Entity<T>,
    event: &InputEvent,
    cx: &mut Context<ClipPalette>,
) {
    if matches!(event, InputEvent::Change) {
        palette.form_edited(cx);
    }
}

impl Form {
    pub fn new(
        target: Option<ClipId>,
        initial: Initial,
        (scopes, scope): (Vec<Scope>, usize),
        (window, cx): (&mut Window, &mut Context<ClipPalette>),
    ) -> Self {
        let title = text_field_state(tr("clip-form-title-hint"), window, cx);
        let user = text_field_state(tr("clip-form-user-hint"), window, cx);
        let body = cx.new(|cx| plain_editor(window, cx));
        title.update(cx, |field, cx| {
            field.set_value(initial.title.clone(), window, cx)
        });
        user.update(cx, |field, cx| {
            field.set_value(initial.user.clone(), window, cx)
        });
        body.update(cx, |editor, cx| {
            editor.set_value(initial.body.clone(), window, cx)
        });
        let changes = [
            cx.subscribe(&title, on_edit),
            cx.subscribe(&user, on_edit),
            cx.subscribe(&body, on_edit),
        ];
        Self {
            target,
            was_secret: target.is_some() && initial.kind == FormKind::Secret,
            kind: initial.kind,
            scopes,
            scope,
            title,
            user,
            body,
            password: Password::new(cx),
            kind_focus: cx.focus_handle(),
            scope_focus: cx.focus_handle(),
            start: Start {
                kind: initial.kind,
                title: initial.title,
                body: initial.body,
                user: initial.user,
                scope,
            },
            problem: None,
            is_asking: false,
            _changes: changes,
        }
    }

    /// An existing secret keeps its password when the field stays empty.
    pub fn keeps_password(&self) -> bool {
        self.was_secret
    }

    pub fn is_new(&self) -> bool {
        self.target.is_none()
    }

    pub fn set_kind(&mut self, kind: FormKind) {
        if kind == self.kind {
            return;
        }
        if self.kind == FormKind::Secret {
            self.password.clear();
        }
        self.kind = kind;
        self.problem = None;
    }

    pub fn handle(&self, field: Field, cx: &App) -> FocusHandle {
        match field {
            Field::Kind => self.kind_focus.clone(),
            Field::Title => self.title.focus_handle(cx),
            Field::Body => self.body.focus_handle(cx),
            Field::User => self.user.focus_handle(cx),
            Field::Password => self.password.focus().clone(),
            Field::Scope => self.scope_focus.clone(),
        }
    }

    /// The fields in Tab order for the chosen kind.
    pub fn order(&self) -> Vec<Field> {
        let mut order = vec![Field::Kind, Field::Title];
        if self.kind == FormKind::Secret {
            order.extend([Field::User, Field::Password]);
        } else {
            order.push(Field::Body);
        }
        if self.scopes.len() > 1 {
            order.push(Field::Scope);
        }
        order
    }

    pub fn focused(&self, window: &Window, cx: &App) -> Option<Field> {
        let is_focused = |field: &Field| self.handle(*field, cx).is_focused(window);
        self.order().into_iter().find(is_focused)
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        let start = &self.start;
        let text = |value: gpui_kit::SharedString| value.to_string();
        self.kind != start.kind
            || self.scope != start.scope
            || !self.password.is_empty()
            || text(self.title.read(cx).value()) != start.title
            || text(self.user.read(cx).value()) != start.user
            || text(self.body.read(cx).value()) != start.body
    }

    fn draft(&self, cx: &App) -> Result<SavedDraft, Problem> {
        let title = self.title.read(cx).value().trim().to_owned();
        if title.is_empty() {
            return Err(Problem::NoTitle);
        }
        if title.chars().count() > TITLE_MAX {
            return Err(Problem::TitleTooLong);
        }
        let is_secret = self.kind == FormKind::Secret;
        let body = if is_secret {
            if self.password.is_empty() && !self.keeps_password() {
                return Err(Problem::NoPassword);
            }
            self.password.secret()
        } else {
            let text = self.body.read(cx).value().to_string();
            if text.is_empty() && self.was_secret {
                return Err(Problem::NoBody);
            }
            SecretText::new(text)
        };
        let user = is_secret.then(|| self.user.read(cx).value().trim().to_owned());
        Ok(SavedDraft {
            kind: self.kind.clip_kind(),
            title,
            body,
            user,
            scope: self.scopes[self.scope].clone(),
        })
    }

    /// Validates and sends the draft; the draft's buffer is wiped when it is dropped.
    pub fn submit(&self, hooks: &dyn ClipboardHooks, cx: &App) -> Result<(), Problem> {
        let draft = self.draft(cx)?;
        let sent = match self.target {
            Some(id) => hooks.update_saved(id, draft),
            None => hooks.save(draft).map(|_| ()),
        };
        sent.map_err(Problem::Refused)
    }
}
