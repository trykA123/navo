//! User-defined commands read from `user-commands.toml`.

#![cfg_attr(
    not(test),
    expect(dead_code, reason = "Windows uses the remaining liveness states")
)]

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use navo_core::Location;
use navo_fs::Watch;
use navo_store::StoreDirs;
use navo_ui::window_view::{
    UserCommandHooks, UserCommandInfo, UserCommandNews, UserCommandSender, UserInvocation,
};

use crate::editor::Search;
use crate::watch_file::watch_file;

mod expand;
mod model;
mod program;
mod start;

#[cfg(test)]
mod tests;

pub const FILE: &str = "user-commands.toml";

type Run = Arc<dyn Fn(&str, UserInvocation) + Send + Sync>;

const TEMPLATE: &str = "\
# User commands for Navo.
# Each [[command]] table is one command; uncomment and edit the example.
#
# [[command]]
# name = \"Open in Notepad\"
# program = \"notepad.exe\"
# args = \"{file}\"
# start-in = \"{folder}\"
# console = \"show\"
# key = \"ctrl-alt-e\"
";

/// Loads the command file, watches it and sweeps old list files before returning.
#[must_use]
pub fn hooks(dirs: Option<&StoreDirs>) -> (UserCommandHooks, Option<Watch>) {
    let Some(dirs) = dirs.filter(|dirs| dirs.ensure().is_ok()) else {
        return (no_hooks(), None);
    };
    let path = dirs.user_commands_file();
    let loaded = load(&path);
    report(&loaded.news);
    let commands = Arc::new(Mutex::new(loaded.commands.unwrap_or_default()));
    let sender_slot = Arc::new(Mutex::new(None));
    let run = run_hook(Arc::clone(&commands), Arc::clone(&sender_slot));
    let (hooks, sender) = UserCommandHooks::channel(run, location_of(&path));
    let template = path.clone();
    let hooks = hooks.with_edit(Arc::new(move || {
        edit(&template).map_err(|error| format!("{FILE}: {error}"))
    }));
    *lock(&sender_slot) = Some(sender.clone());
    // Watch before the first news, so an edit made as soon as it arrives is seen.
    let watching = watch(&path, loaded.news.clone(), commands, sender.clone());
    let _ = sender.send(loaded.news);
    start::sweep(SystemTime::now(), start::is_running);
    if watching.is_none() {
        tracing::warn!(
            "the settings folder is not watched; user-commands.toml changes apply at the next start"
        );
    }
    (hooks, watching)
}

/// Writes the commented template only when the file is absent.
pub fn edit(path: &Path) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    match options.open(path) {
        Ok(mut file) => file.write_all(TEMPLATE.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

struct Loaded {
    news: UserCommandNews,
    commands: Option<Vec<model::UserCommand>>,
}

fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Loaded {
                news: UserCommandNews::Invalid(format!("{FILE}: {error}")),
                commands: None,
            };
        }
    };
    match model::parse(&text, &search()) {
        Ok(commands) => {
            let list = commands
                .iter()
                .map(|command| UserCommandInfo {
                    name: command.name.clone(),
                    key: command.key.clone(),
                })
                .collect();
            Loaded {
                news: UserCommandNews::Applied(list, Vec::new()),
                commands: Some(commands),
            }
        }
        Err(errors) => Loaded {
            news: UserCommandNews::Invalid(
                errors
                    .iter()
                    .map(|error| format!("{FILE}, {error}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            commands: None,
        },
    }
}

fn watch(
    path: &Path,
    start: UserCommandNews,
    commands: Arc<Mutex<Vec<model::UserCommand>>>,
    sender: UserCommandSender,
) -> Option<Watch> {
    let file = path.to_owned();
    let mut last = start;
    watch_file(path, move || {
        let Loaded {
            news,
            commands: next,
        } = load(&file);
        if let Some(next) = next {
            *lock(&commands) = next;
        }
        if news == last {
            return;
        }
        last = news.clone();
        report(&news);
        let _ = sender.send(news);
    })
}

fn run_hook(
    commands: Arc<Mutex<Vec<model::UserCommand>>>,
    sender: Arc<Mutex<Option<UserCommandSender>>>,
) -> Run {
    Arc::new(move |name, invocation| {
        let name = name.to_owned();
        let worker_commands = Arc::clone(&commands);
        let worker_sender = Arc::clone(&sender);
        let failure_name = name.clone();
        let failure_sender = Arc::clone(&sender);
        let spawned = std::thread::Builder::new()
            .name("navo-user-command".to_owned())
            .spawn(move || run_one(&name, invocation, &worker_commands, &worker_sender));
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start the user command thread");
            if let Some(sender) = lock(&failure_sender).clone() {
                let _ = sender.send(UserCommandNews::Refused {
                    name: failure_name,
                    message: format!("could not start the command: {error}"),
                });
            }
        }
    })
}

fn run_one(
    name: &str,
    invocation: UserInvocation,
    commands: &Mutex<Vec<model::UserCommand>>,
    sender: &Mutex<Option<UserCommandSender>>,
) {
    let Some(sender) = lock(sender).clone() else {
        return;
    };
    let command = lock(commands)
        .iter()
        .find(|command| command.name == name)
        .cloned();
    let Some(command) = command else {
        let _ = sender.send(UserCommandNews::Refused {
            name: name.to_owned(),
            message: format!("there is no user command named `{name}`"),
        });
        return;
    };
    let invocation = expand::Invocation {
        file: invocation.file,
        marked: invocation.marked,
        folder: invocation.folder,
        other_folder: invocation.other_folder,
        other_file: invocation.other_file,
    };
    let plan = match expand::plan(&command, &invocation, &search()) {
        Ok(plan) => plan,
        Err(error) => {
            let _ = sender.send(UserCommandNews::Refused {
                name: name.to_owned(),
                message: error.to_string(),
            });
            return;
        }
    };
    let exit_sender = sender.clone();
    let exit_name = name.to_owned();
    let hidden = plan.console == model::Console::Hide;
    let started = start::start(
        plan,
        Box::new(move |code| {
            let _ = exit_sender.send(UserCommandNews::Exited {
                name: exit_name,
                code,
                hidden,
            });
        }),
    );
    if let Err(message) = started {
        let _ = sender.send(UserCommandNews::Refused {
            name: name.to_owned(),
            message,
        });
    }
}

fn report(news: &UserCommandNews) {
    match news {
        UserCommandNews::Applied(_, warnings) => {
            for warning in warnings {
                tracing::warn!(problem = %warning, "user commands applied with a warning");
            }
        }
        UserCommandNews::Invalid(message) => {
            tracing::error!(problem = %message, "user commands not applied");
        }
        UserCommandNews::Exited { .. } | UserCommandNews::Refused { .. } => {}
    }
}

fn search() -> Search {
    Search {
        path: std::env::var("PATH").unwrap_or_default(),
        extensions: std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned()),
    }
}

fn location_of(path: &Path) -> Option<Location> {
    Location::new(path.to_str()?).ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn no_hooks() -> UserCommandHooks {
    UserCommandHooks {
        run: Arc::new(|_, _| {}),
        edit: Arc::new(|| Ok(())),
        file: None,
        news: None,
    }
}
