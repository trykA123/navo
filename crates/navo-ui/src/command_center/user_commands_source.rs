use gpui_kit::SharedString;
use navo_index::fuzzy::Fuzzy;
use navo_l10n::tr;

use super::commands_source::words;
use super::rank;
use super::source::{Cancel, Group, Hit, HitSink, Request, ResultSource, Scopes, Target};

/// The user commands of the window at the moment Ctrl+K opens: name and key text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserCommandsSource {
    commands: Vec<(String, Option<String>)>,
}

impl UserCommandsSource {
    #[must_use]
    pub fn new(commands: Vec<(String, Option<String>)>) -> Self {
        Self { commands }
    }
}

impl ResultSource for UserCommandsSource {
    fn id(&self) -> &'static str {
        "user-commands"
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
        for (name, key) in &self.commands {
            if cancel.is_cancelled() {
                return;
            }
            let Some(found) = fuzzy.score(name) else {
                continue;
            };
            sink.push(
                request.generation,
                Hit {
                    source: "user-commands",
                    group: Group::Commands,
                    title: SharedString::from(name.clone()),
                    detail: SharedString::from(tr("user-command-detail")),
                    score: rank::score(i64::from(found.score), 0.0, rank::name_bonus(name, &text)),
                    indices: found.indices,
                    target: Target::User(SharedString::from(name.clone())),
                    shortcut: key.clone().map(SharedString::from),
                },
            );
        }
    }
}
