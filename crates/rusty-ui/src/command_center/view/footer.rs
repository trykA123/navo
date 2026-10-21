use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use rusty_l10n::{t, tr};

use super::super::query::{Query, Scope};
use super::super::results::Row;
use super::super::source::{Group, Hit, Target};
use crate::commands::{ArgKind, Keymap};
use crate::primitives::{IconName, Kbd, icon};
use crate::tokens::Theme;

const HEIGHT: f32 = 30.;
const SEPARATOR: &str = " · ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hint {
    pub keys: &'static str,
    pub command: &'static str,
    pub label: &'static str,
}

const fn hint(keys: &'static str, command: &'static str, label: &'static str) -> Hint {
    Hint {
        keys,
        command,
        label,
    }
}

const RUN_COMMAND: &str = "command_center.run";
const NEXT_COMMAND: &str = "command_center.next_group";
pub(super) const BACK_COMMAND: &str = "command_center.back";

const RUN: Hint = hint("enter", RUN_COMMAND, "command-center-key-run");
const CHOOSE: Hint = hint("enter", RUN_COMMAND, "command-center-key-choose");
const OPEN: Hint = hint("enter", RUN_COMMAND, "command-center-key-open");
const GO: Hint = hint("enter", RUN_COMMAND, "command-center-key-go");
const SHOW_ALL: Hint = hint("enter", RUN_COMMAND, "command-center-key-show-all");
const REVEAL: Hint = hint(
    "shift-enter",
    "command_center.reveal",
    "command-center-key-reveal",
);
const OTHER_PANE: Hint = hint(
    "ctrl-enter",
    "command_center.other_pane",
    "command-center-key-other-pane",
);
const NEW_TAB: Hint = hint(
    "alt-enter",
    "command_center.run_in_new_tab",
    "command-center-key-new-tab",
);
const NEXT_GROUP: Hint = hint("tab", NEXT_COMMAND, "command-center-key-next-group");
const COMPLETE: Hint = hint("tab", NEXT_COMMAND, "command-center-key-complete");
const CLOSE: Hint = hint("escape", BACK_COMMAND, "command-center-key-close");
const BACK: Hint = hint("escape", BACK_COMMAND, "command-center-key-back");

const HINTED: [&str; 6] = [
    RUN_COMMAND,
    "command_center.run_in_new_tab",
    NEXT_COMMAND,
    BACK_COMMAND,
    "command_center.reveal",
    "command_center.other_pane",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintKeys(Rc<[(&'static str, SharedString)]>);

impl HintKeys {
    #[must_use]
    pub fn of(keymap: &Keymap) -> Self {
        let keys = HINTED
            .iter()
            .filter_map(|command| {
                let key = keymap.keys_of(command).next()?;
                Some((*command, SharedString::from(key.to_owned())))
            })
            .collect();
        Self(keys)
    }

    #[must_use]
    pub fn key(&self, command: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(name, _)| *name == command)
            .map(|(_, key)| key.as_ref())
    }
}

pub(crate) fn hints(row: Option<&Row>, argument: Option<ArgKind>, is_path: bool) -> Vec<Hint> {
    match (argument, row) {
        (Some(ArgKind::Text), _) => vec![RUN, BACK],
        (Some(ArgKind::Path), _) => vec![GO, COMPLETE, BACK],
        (None, Some(Row::ShowAll { .. })) => vec![SHOW_ALL, NEXT_GROUP],
        (None, Some(Row::TopHit(hit) | Row::Hit(hit))) => hit_hints(hit, is_path),
        (None, _) => vec![CLOSE],
    }
}

fn hit_hints(hit: &Hit, is_path: bool) -> Vec<Hint> {
    match hit.target {
        Target::Open { .. } | Target::GoTo(_) if is_path => {
            vec![OPEN, COMPLETE, REVEAL, OTHER_PANE, NEW_TAB]
        }
        Target::Open { .. } | Target::GoTo(_) => vec![OPEN, REVEAL, OTHER_PANE, NEW_TAB],
        Target::ShowAll(_) => vec![SHOW_ALL, NEXT_GROUP],
        Target::Arg(..) => vec![CHOOSE, NEXT_GROUP, CLOSE],
        Target::Action(_) | Target::Core(_) => vec![RUN, NEXT_GROUP, CLOSE],
    }
}

pub(crate) fn result_count(rows: &[Row]) -> usize {
    let top_group = rows.iter().find_map(|row| match row {
        Row::TopHit(hit) => Some(hit.group),
        _ => None,
    });
    let shown = |group: Group| {
        rows.iter()
            .filter(|row| matches!(row, Row::Hit(hit) if hit.group == group))
            .count()
            + usize::from(top_group == Some(group))
    };
    rows.iter()
        .map(|row| match row {
            Row::TopHit(_) | Row::Hit(_) => 1,
            Row::ShowAll { group, total } => total.saturating_sub(shown(*group)),
            Row::Header(_) | Row::Hint => 0,
        })
        .sum()
}

pub(super) fn empty_parts(query: &str, is_path: bool) -> (String, String) {
    let parsed = Query::parse(query);
    let (id, term) = match (is_path || parsed.path.is_some(), parsed.scope) {
        (true, _) => ("command-center-none-path", query),
        (false, Scope::Mixed) => ("command-center-none-mixed", query),
        (false, Scope::Commands) => ("command-center-none-commands", &query[1..]),
        (false, Scope::Files) => ("command-center-none-files", &query[1..]),
    };
    (tr(id), term.trim().to_owned())
}

pub(super) fn empty_message(t: &Theme, query: &str, is_path: bool) -> impl IntoElement {
    let (lead, term) = empty_parts(query, is_path);
    div()
        .id("command-center-empty")
        .role(Role::Status)
        .aria_label(format!("{lead} {term}."))
        .test_support()
        .flex()
        .flex_wrap()
        .gap(t.spacing.xs)
        .px(t.spacing.md)
        .py(t.spacing.xl)
        .border_t_1()
        .border_color(t.colors.line)
        .text_color(t.colors.muted)
        .child(lead)
        .child(
            div()
                .text_color(t.colors.text)
                .font_weight(FontWeight::MEDIUM)
                .child(format!("{term}.")),
        )
}

pub(super) struct FooterLook<'a> {
    pub theme: &'a Theme,
    pub note: Option<SharedString>,
    pub error: Option<SharedString>,
    pub count: Option<usize>,
    pub hints: Option<(Vec<Hint>, &'a HintKeys)>,
}

fn status(t: &Theme, look: &FooterLook) -> Option<Div> {
    if let Some(error) = look.error.clone() {
        let line = div()
            .id("command-center-error")
            .role(Role::Alert)
            .aria_label(error.clone())
            .test_support()
            .min_w_0()
            .truncate()
            .text_color(t.colors.mark)
            .child(error);
        return Some(div().min_w_0().child(line));
    }
    let (text, is_note) = match (look.note.clone(), look.count) {
        (Some(note), _) => (note, true),
        (None, Some(count)) => (t!("command-center-results", count = count).into(), false),
        (None, None) => return None,
    };
    let line = div()
        .id("command-center-note")
        .role(Role::Status)
        .aria_label(text.clone())
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .min_w_0()
        .when(is_note, |line| {
            line.child(icon(IconName::Loader, t.spacing.icon, t.colors.muted))
        })
        .child(div().min_w_0().truncate().child(text));
    Some(div().min_w_0().child(line))
}

fn key_hints(t: &Theme, hints: &[Hint], keys: &HintKeys) -> impl IntoElement {
    let caps: Vec<_> = hints
        .iter()
        .filter_map(|hint| Some((keys.key(hint.command)?, hint)))
        .enumerate()
        .map(|(index, (key, hint))| (Kbd::new(("command-center-hint-key", index), key), hint))
        .collect();
    let label = caps
        .iter()
        .map(|(key, hint)| {
            let key = key.as_ref().map(Kbd::text).unwrap_or_default();
            format!("{key} {}", tr(hint.label))
        })
        .collect::<Vec<_>>()
        .join(SEPARATOR);
    div()
        .id("command-center-hints")
        .role(Role::Group)
        .aria_label(label)
        .test_support()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.xl)
        .ml_auto()
        .children(caps.into_iter().map(|(key, hint)| {
            div()
                .flex()
                .items_center()
                .gap(t.spacing.xs)
                .children(key)
                .child(tr(hint.label))
        }))
}

pub(super) fn render(look: FooterLook) -> Div {
    let t = look.theme;
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(t.spacing.xl)
        .h(px(HEIGHT))
        .px(t.spacing.xl)
        .border_t_1()
        .border_color(t.colors.line)
        .bg(t.colors.rail)
        .text_size(t.typography.tiny)
        .text_color(t.colors.muted)
        .whitespace_nowrap()
        .children(status(t, &look))
        .when_some(look.hints.as_ref(), |footer, (hints, keys)| {
            footer.child(key_hints(t, hints, keys))
        })
}
