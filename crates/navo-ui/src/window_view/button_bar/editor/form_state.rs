//! Form state, debounced update dispatch, and pure helpers for the button bar editor form.

use std::time::Duration;

use gpui_kit::component::input::InputEvent;
use gpui_kit::{App, AppContext, Context, Entity, Subscription, Task, Window};
use navo_l10n::tr;

use crate::commands;
use crate::primitives::{ListScroll, TextFieldState, text_field_state};
use crate::window_view::NavoWindow;
use crate::window_view::button_bar::view;
use crate::window_view::button_bar::{
    BarEdit, ButtonAction, ButtonItem, DraftAction, ItemDraft, ItemPath,
};

const DEBOUNCE_DELAY: Duration = Duration::from_millis(400);

/// Active action kind tab in the button form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum FormKind {
    User,
    #[default]
    Navo,
    Program,
}

/// Persistent inputs, debounce timer, and validation error for the button editor form.
pub(crate) struct FormState {
    pub(super) kind: FormKind,
    pub(super) name_input: Entity<TextFieldState>,
    pub(super) navo_filter: Entity<TextFieldState>,
    pub(super) program_input: Entity<TextFieldState>,
    pub(super) args_input: Entity<TextFieldState>,
    pub(super) start_in_input: Entity<TextFieldState>,
    pub(super) user_name: String,
    pub(super) navo_id: String,
    pub(super) hidden_console: bool,
    pub(super) user_scroll: ListScroll,
    pub(super) navo_scroll: ListScroll,
    pub(super) error: Option<String>,
    debounce: Option<Task<()>>,
    syncing: bool,
    pub(crate) force_resync: bool,
    last_synced_path: Option<ItemPath>,
    last_submitted: Option<(ItemPath, ItemDraft)>,
    _subs: Vec<Subscription>,
}

impl FormState {
    /// Allocates form text inputs and registers change listeners on `cx`.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<NavoWindow>) -> Self {
        let name_input = text_field_state(tr("button-bar-form-name-placeholder"), window, cx);
        let navo_filter = text_field_state(tr("button-bar-form-navo-filter"), window, cx);
        let program_input = text_field_state(tr("button-bar-form-program-placeholder"), window, cx);
        let args_input = text_field_state(tr("button-bar-form-args-placeholder"), window, cx);
        let start_in_input =
            text_field_state(tr("button-bar-form-start-in-placeholder"), window, cx);

        let mut subs = Vec::with_capacity(5);
        for input in [&name_input, &program_input, &args_input, &start_in_input] {
            subs.push(subscribe_text_field(input, window, cx));
        }
        subs.push(
            cx.subscribe(&navo_filter, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.form_navo_filter_changed(cx);
                }
            }),
        );

        Self {
            kind: FormKind::default(),
            name_input,
            navo_filter,
            program_input,
            args_input,
            start_in_input,
            user_name: String::new(),
            navo_id: String::new(),
            hidden_console: false,
            user_scroll: ListScroll::new(),
            navo_scroll: ListScroll::new(),
            error: None,
            debounce: None,
            syncing: false,
            force_resync: false,
            last_synced_path: None,
            last_submitted: None,
            _subs: subs,
        }
    }

    /// Populates form state from `item` at `path`, cancelling pending edits.
    pub(crate) fn sync_from_item(
        &mut self,
        path: Option<ItemPath>,
        item: Option<&ButtonItem>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.debounce = None;
        self.error = None;
        self.last_submitted = None;
        self.force_resync = false;
        self.last_synced_path = path;
        self.syncing = true;

        let name = item
            .filter(|entry| !matches!(entry.action, ButtonAction::Separator))
            .map(|entry| entry.name.clone())
            .unwrap_or_default();
        self.name_input
            .update(cx, |state, cx| state.set_value(name, window, cx));

        let (prog, args) = self.apply_action_fields(item.map(|entry| &entry.action));
        self.program_input
            .update(cx, |state, cx| state.set_value(prog, window, cx));
        self.args_input
            .update(cx, |state, cx| state.set_value(args, window, cx));
        self.start_in_input
            .update(cx, |state, cx| state.set_value(String::new(), window, cx));
        self.syncing = false;
    }

    fn apply_action_fields(&mut self, action: Option<&ButtonAction>) -> (String, String) {
        self.user_name.clear();
        self.navo_id.clear();
        self.hidden_console = false;
        match action {
            Some(ButtonAction::User(name)) => {
                self.kind = FormKind::User;
                self.user_name = name.clone();
                (String::new(), String::new())
            }
            Some(ButtonAction::Navo(id)) => {
                self.kind = FormKind::Navo;
                self.navo_id = id.clone();
                (String::new(), String::new())
            }
            Some(ButtonAction::Program { line, .. }) => {
                self.kind = FormKind::Program;
                split_program_line(line)
            }
            _ => (String::new(), String::new()),
        }
    }

    /// Returns `true` when `item` at `path` matches the form's current or submitted state.
    pub(crate) fn should_keep_on_news(
        &self,
        path: Option<ItemPath>,
        item: Option<&ButtonItem>,
    ) -> bool {
        if self.force_resync || path != self.last_synced_path {
            return false;
        }
        let (Some(at), Some(item)) = (path, item) else {
            return path.is_none() && item.is_none();
        };
        self.last_submitted
            .as_ref()
            .is_some_and(|(sub_at, draft)| *sub_at == at && draft_matches_item(draft, item))
    }
}

impl NavoWindow {
    fn form_navo_filter_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_ref() {
            editor.form.navo_scroll.scroll_to_top(0);
        }
        cx.notify();
    }

    fn form_text_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        if editor.form.syncing {
            return;
        }
        editor.form.error = None;
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE_DELAY).await;
            let _ = this.update_in(cx, |view, window, cx| {
                view.flush_form_update(window, cx);
            });
        });
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.form.debounce = Some(task);
        }
        cx.notify();
    }

    fn flush_form_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.form.debounce = None;
        }
        self.submit_form_update(window, cx);
    }

    pub(super) fn select_form_kind(
        &mut self,
        kind: FormKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_submit = {
            let Some(editor) = self.button_bar.editor.as_mut() else {
                return;
            };
            editor.form.kind = kind;
            editor.form.error = None;
            editor.form.debounce = None;
            match kind {
                FormKind::User => !editor.form.user_name.is_empty(),
                FormKind::Navo => !editor.form.navo_id.is_empty(),
                FormKind::Program => !editor.form.program_input.read(cx).value().trim().is_empty(),
            }
        };
        if should_submit {
            self.submit_form_update(window, cx);
        } else {
            cx.notify();
        }
    }

    pub(super) fn pick_user_command(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.form.kind = FormKind::User;
            editor.form.user_name = name;
            editor.form.navo_id.clear();
            editor.form.error = None;
            editor.form.debounce = None;
        }
        self.submit_form_update(window, cx);
    }

    pub(super) fn pick_navo_command(
        &mut self,
        id: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.form.kind = FormKind::Navo;
            editor.form.navo_id = id.to_owned();
            editor.form.user_name.clear();
            editor.form.error = None;
            editor.form.debounce = None;
        }
        self.submit_form_update(window, cx);
    }

    pub(super) fn set_form_hidden_console(
        &mut self,
        hidden: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.button_bar.editor.as_mut() {
            editor.form.hidden_console = hidden;
            editor.form.error = None;
            editor.form.debounce = None;
        }
        self.submit_form_update(window, cx);
    }

    fn submit_form_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.button_bar.editor.as_ref().and_then(|e| e.selected) else {
            return;
        };
        let is_menu = self
            .selected_item(Some(at))
            .is_some_and(|item| matches!(item.action, ButtonAction::Menu(_)));
        let Some(edit_fn) = self.button_bar.hooks.as_ref().map(|h| h.edit.clone()) else {
            return;
        };
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        let draft = build_draft(&editor.form, is_menu, cx);
        editor.form.last_submitted = Some((at, draft.clone()));
        editor.form.error = None;
        editor.error = None;
        let request = BarEdit::Update { at, draft };
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { edit_fn(request) }).await;
            let _ = this.update_in(cx, |view, _, cx| view.finish_form_update(result, cx));
        })
        .detach();
    }

    fn finish_form_update(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        let Some(editor) = self.button_bar.editor.as_mut() else {
            return;
        };
        match result {
            Ok(()) => editor.form.error = None,
            Err(message) => {
                editor.form.last_submitted = None;
                editor.form.error = Some(message);
            }
        }
        cx.notify();
    }
}

fn subscribe_text_field(
    input: &Entity<TextFieldState>,
    window: &mut Window,
    cx: &mut Context<NavoWindow>,
) -> Subscription {
    cx.subscribe_in(input, window, |view, _, event: &InputEvent, window, cx| {
        if matches!(event, InputEvent::Change) {
            view.form_text_changed(window, cx);
        }
    })
}

fn build_draft(form: &FormState, is_menu: bool, cx: &App) -> ItemDraft {
    let name = form.name_input.read(cx).value().to_string();
    let action = if is_menu {
        DraftAction::Menu
    } else {
        match form.kind {
            FormKind::User => DraftAction::User(form.user_name.clone()),
            FormKind::Navo => DraftAction::Navo(form.navo_id.clone()),
            FormKind::Program => DraftAction::Program {
                program: form.program_input.read(cx).value().to_string(),
                args: form.args_input.read(cx).value().to_string(),
                start_in: form.start_in_input.read(cx).value().to_string(),
                hidden_console: form.hidden_console,
            },
        }
    };
    ItemDraft {
        name,
        action,
        icon: None,
    }
}

/// Returns runnable Navo command `(id, label)` pairs matching all words in `query`.
pub(super) fn filter_navo_commands(query: &str) -> Vec<(&'static str, String)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    commands::all()
        .filter(|spec| view::runnable(spec.name).is_some())
        .filter_map(|spec| {
            let label = tr(spec.label);
            if words.is_empty() {
                return Some((spec.name, label));
            }
            let lower_label = label.to_lowercase();
            let lower_id = spec.name.to_lowercase();
            let matches = words
                .iter()
                .all(|word| lower_label.contains(word) || lower_id.contains(word));
            matches.then_some((spec.name, label))
        })
        .collect()
}

/// Returns `true` when `args` contains a file placeholder (`{file}`, `{name}`, `{stem}`, `{ext}`, `{marked}`, `{marked-list}`).
pub(super) fn args_takes_files(args: &str) -> bool {
    const TOKENS: [&str; 6] = ["file}", "name}", "stem}", "ext}", "marked}", "marked-list}"];
    let bytes = args.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if bytes.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            let rest = &args[i + 1..];
            if TOKENS.iter().any(|token| rest.starts_with(token)) {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn split_program_line(line: &str) -> (String, String) {
    let trimmed = line.trim();
    if let Some(after_open) = trimmed.strip_prefix('"')
        && let Some(end_quote) = after_open.find('"')
    {
        let program = trimmed[..end_quote + 2].to_owned();
        let args = after_open[end_quote + 1..].trim_start().to_owned();
        return (program, args);
    }
    match trimmed.split_once(' ') {
        Some((program, args)) => (program.to_owned(), args.trim_start().to_owned()),
        None => (trimmed.to_owned(), String::new()),
    }
}

fn draft_matches_item(draft: &ItemDraft, item: &ButtonItem) -> bool {
    if draft.name != item.name {
        return false;
    }
    match (&draft.action, &item.action) {
        (DraftAction::Navo(a), ButtonAction::Navo(b)) => a == b,
        (DraftAction::User(a), ButtonAction::User(b)) => a == b,
        (DraftAction::Program { program, args, .. }, ButtonAction::Program { line, .. }) => {
            let expected = if args.is_empty() {
                program.clone()
            } else {
                format!("{program} {args}")
            };
            expected == *line
        }
        (DraftAction::Menu, ButtonAction::Menu(_))
        | (DraftAction::Separator, ButtonAction::Separator) => true,
        _ => false,
    }
}
