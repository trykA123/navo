use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use gpui_kit::SharedString;
use rusty_index::fuzzy::Fuzzy;
use rusty_l10n::tr;

use super::context_name::context_name;
use super::query::{Scope, Term};
use super::rank;
use super::source::{Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target};
use crate::commands::{ArgKind, CommandEntry, GLOBAL, Keymap, is_within, shown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRow {
    pub name: &'static str,
    pub action: &'static str,
    pub label: String,
    pub context: String,
    pub shortcut: Option<String>,
    pub arg: Option<ArgKind>,
}

impl CommandRow {
    fn hit(&self, score: i64, indices: Vec<u32>) -> Hit {
        Hit {
            source: "commands",
            group: Group::Commands,
            title: SharedString::from(self.label.clone()),
            detail: SharedString::from(self.context.clone()),
            score,
            indices,
            target: self.arg.map_or(Target::Action(self.action), |kind| {
                Target::Arg(self.name, kind)
            }),
            shortcut: self.shortcut.clone().map(SharedString::from),
        }
    }

    fn use_key(&self) -> &'static str {
        if self.arg.is_some() {
            self.name
        } else {
            self.action
        }
    }
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
                    context: context_name(entry.spec.context)
                        .unwrap_or_else(|| entry.spec.context.to_owned()),
                    shortcut: shortcut_of(entry, stack, keymap),
                    arg: entry.arg,
                })
            })
            .collect();
        Self { rows }
    }

    #[must_use]
    pub fn hits_of(&self, names: &[String]) -> Vec<Hit> {
        names
            .iter()
            .filter_map(|name| self.rows.iter().find(|row| row.use_key() == name))
            .map(|row| row.hit(0, Vec::new()))
            .collect()
    }

    #[must_use]
    pub fn rows(&self) -> &[CommandRow] {
        &self.rows
    }
}

fn shortcut_of(entry: &CommandEntry, stack: &[&str], keymap: &Keymap) -> Option<String> {
    let mut binds: Vec<_> = keymap
        .binds()
        .iter()
        .filter(|bind| bind.spec.name == entry.spec.name && in_scope(&bind.context, stack))
        .collect();
    binds.sort_by_key(|bind| bind.context != entry.spec.context);
    binds
        .into_iter()
        .flat_map(|bind| bind.keys.iter())
        .find(|key| shown(key).is_some())
        .cloned()
}

fn in_scope(context: &str, stack: &[&str]) -> bool {
    context == GLOBAL || stack.iter().any(|at| is_within(at, context))
}

#[derive(Clone)]
pub struct CommandsSource {
    snapshot: Arc<CommandsSnapshot>,
    uses: Option<UseWeights>,
    weights: Arc<OnceLock<HashMap<String, f64>>>,
}

pub type UseWeights = Arc<dyn Fn() -> Vec<(String, f64)> + Send + Sync>;

impl std::fmt::Debug for CommandsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandsSource")
            .field("snapshot", &self.snapshot)
            .finish_non_exhaustive()
    }
}

impl CommandsSource {
    #[must_use]
    pub fn new(snapshot: Arc<CommandsSnapshot>) -> Self {
        Self {
            snapshot,
            uses: None,
            weights: Arc::default(),
        }
    }

    #[must_use]
    pub fn with_use(snapshot: Arc<CommandsSnapshot>, uses: UseWeights) -> Self {
        Self {
            snapshot,
            uses: Some(uses),
            weights: Arc::default(),
        }
    }

    fn weights(&self) -> &HashMap<String, f64> {
        self.weights.get_or_init(|| {
            self.uses
                .as_ref()
                .map(|load| load().into_iter().collect())
                .unwrap_or_default()
        })
    }
}

pub(super) fn words(request: &Request) -> Option<String> {
    if request.query.path.is_some() {
        return None;
    }
    let scoped = request.query.scope == Scope::Commands;
    let mut words = Vec::new();
    for term in &request.query.terms {
        match term {
            Term::Name(text) => words.push(text.as_str()),
            Term::Path(text) if scoped => words.push(text.as_str()),
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
        let weights = self.weights();
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
            let hit = row.hit(
                rank::score(
                    i64::from(score),
                    weights.get(row.use_key()).copied().unwrap_or(0.0),
                    rank::name_bonus(&row.label, &text),
                ),
                indices,
            );
            sink.push(request.generation, hit);
        }
    }
}
