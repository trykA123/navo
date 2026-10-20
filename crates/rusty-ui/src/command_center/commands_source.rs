use std::sync::Arc;

use gpui_kit::SharedString;
use rusty_index::fuzzy::Fuzzy;
use rusty_l10n::tr;

use super::query::Term;
use super::source::{Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target};
use crate::commands::{ArgKind, CommandEntry, GLOBAL, Keymap, is_within, shown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRow {
    pub name: &'static str,
    pub action: &'static str,
    pub label: String,
    pub shortcut: Option<String>,
    pub arg: Option<ArgKind>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandsSnapshot {
    rows: Vec<CommandRow>,
}

impl CommandsSnapshot {
    #[must_use]
    pub fn build(entries: &[CommandEntry], stack: &[&str], keymap: &Keymap) -> Self {
        let rows = entries
            .iter()
            .filter(|entry| entry.enabled && entry.in_palette)
            .filter(|entry| in_scope(entry.spec.context, stack))
            .filter_map(|entry| {
                let action = entry.spec.action?;
                Some(CommandRow {
                    name: entry.spec.name,
                    action,
                    label: tr(entry.spec.label),
                    shortcut: keymap.keys_of(entry.spec.name).find_map(shown),
                    arg: entry.arg,
                })
            })
            .collect();
        Self { rows }
    }

    #[must_use]
    pub fn rows(&self) -> &[CommandRow] {
        &self.rows
    }
}

fn in_scope(context: &str, stack: &[&str]) -> bool {
    context == GLOBAL || stack.iter().any(|at| is_within(at, context))
}

#[derive(Clone, Debug)]
pub struct CommandsSource {
    snapshot: Arc<CommandsSnapshot>,
}

impl CommandsSource {
    #[must_use]
    pub fn new(snapshot: Arc<CommandsSnapshot>) -> Self {
        Self { snapshot }
    }
}

fn words(request: &Request) -> Option<String> {
    if request.query.path.is_some() {
        return None;
    }
    let mut words = Vec::new();
    for term in &request.query.terms {
        match term {
            Term::Name(text) => words.push(text.as_str()),
            Term::Path(_) | Term::Ext(_) => return None,
        }
    }
    Some(words.join(" "))
}

impl ResultSource for CommandsSource {
    fn id(&self) -> &'static str {
        "commands"
    }

    fn group(&self) -> Group {
        Group::Commands
    }

    fn scopes(&self) -> Scopes {
        Scopes::COMMANDS
    }

    fn search(&self, request: &Request, sink: &HitSink, cancel: &Cancel) {
        let Some(text) = words(request) else {
            return;
        };
        let mut fuzzy = Fuzzy::new(&text, false);
        for row in &self.snapshot.rows {
            if cancel.is_cancelled() {
                return;
            }
            let (score, indices) = match (fuzzy.score(&row.label), fuzzy.score(row.name)) {
                (Some(label), Some(name)) if name.score > label.score => (name.score, Vec::new()),
                (Some(label), _) => (label.score, label.indices),
                (None, Some(name)) => (name.score, Vec::new()),
                (None, None) => continue,
            };
            let target = row.arg.map_or(Target::Action(row.action), |kind| {
                Target::Arg(row.name, kind)
            });
            let hit = Hit {
                source: "commands",
                group: Group::Commands,
                title: SharedString::from(row.label.clone()),
                detail: SharedString::default(),
                score: i64::from(score),
                indices,
                target,
                shortcut: row.shortcut.clone().map(SharedString::from),
            };
            sink.push(request.generation, hit);
        }
    }
}
