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

mod error;
mod expand;
mod model;
mod program;
mod start;

pub use error::UserCommandError;
pub(crate) use model::UserCommand;

#[cfg(test)]
mod tests;

pub const FILE: &str = "user-commands.toml";

type Run = Arc<dyn Fn(&str, UserInvocation) + Send + Sync>;

/// Starts one expanded plan; `on_exit` gets its exit code later.
type Launch = Arc<
    dyn Fn(expand::Plan, Box<dyn FnOnce(Option<i32>) + Send>) -> Result<(), UserCommandError>
        + Send
        + Sync,
>;

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
pub fn hooks(dirs: Option<&StoreDirs>) -> (UserCommandHooks, Option<Watch>, Runner) {
    let sender_slot = Arc::new(Mutex::new(None));
    let launch: Launch = Arc::new(launch);
    let runner = Runner {
        sender: Arc::clone(&sender_slot),
        launch: Arc::clone(&launch),
    };
    let Some(dirs) = dirs.filter(|dirs| dirs.ensure().is_ok()) else {
        return (no_hooks(), None, runner);
    };
    let path = dirs.user_commands_file();
    let loaded = load(&path);
    report(&loaded.news);
    let commands = Arc::new(Mutex::new(loaded.commands.unwrap_or_default()));
    let run = run_hook(Arc::clone(&commands), Arc::clone(&sender_slot), launch);
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
    (hooks, watching, runner)
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
                    takes_files: model::files::takes_files(&command.args),
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
                    .map(model::LineError::message)
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
    launch: Launch,
) -> Run {
    Arc::new(move |name, invocation| {
        let name = name.to_owned();
        let worker_commands = Arc::clone(&commands);
        let worker_sender = Arc::clone(&sender);
        let worker_launch = Arc::clone(&launch);
        spawn_worker(name.clone(), Arc::clone(&sender), move || {
            run_one(
                &name,
                invocation,
                &worker_commands,
                &worker_sender,
                &worker_launch,
            );
        });
    })
}

/// Starts loaded commands directly, sharing the run hook's news channel.
pub(crate) struct Runner {
    sender: Arc<Mutex<Option<UserCommandSender>>>,
    launch: Launch,
}

impl Runner {
    /// Runs `command` for `invocation` on a worker thread.
    pub(crate) fn run(&self, command: UserCommand, invocation: UserInvocation) {
        let name = command.name.clone();
        let worker_sender = Arc::clone(&self.sender);
        let worker_launch = Arc::clone(&self.launch);
        spawn_worker(name, Arc::clone(&self.sender), move || {
            run_command(command, invocation, &worker_sender, &worker_launch);
        });
    }

    /// Replaces the process launch with `launch`, so a test can record the plan.
    #[cfg(test)]
    pub(crate) fn with_launch(mut self, launch: Launch) -> Self {
        self.launch = launch;
        self
    }
}

/// Spawns `work` on the command thread; a failed spawn is refused as `name`.
fn spawn_worker(
    name: String,
    sender: Arc<Mutex<Option<UserCommandSender>>>,
    work: impl FnOnce() + Send + 'static,
) {
    let failure_sender = Arc::clone(&sender);
    let spawned = std::thread::Builder::new()
        .name("navo-user-command".to_owned())
        .spawn(work);
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the user command thread");
        if let Some(sender) = lock(&failure_sender).clone() {
            refuse(&sender, &name, &UserCommandError::from(error));
        }
    }
}

/// Runs the named command on a worker thread and sends its refusal or exit as news.
fn run_one(
    name: &str,
    invocation: UserInvocation,
    commands: &Mutex<Vec<UserCommand>>,
    sender: &Mutex<Option<UserCommandSender>>,
    launch: &Launch,
) {
    match find_command(name, commands) {
        Ok(command) => run_command(command, invocation, sender, launch),
        Err(error) => {
            if let Some(sender) = lock(sender).clone() {
                refuse(&sender, name, &error);
            }
        }
    }
}

/// Expands `command` for `invocation` and starts it, sending its refusal or exit as news.
fn run_command(
    command: UserCommand,
    invocation: UserInvocation,
    sender: &Mutex<Option<UserCommandSender>>,
    launch: &Launch,
) {
    let Some(sender) = lock(sender).clone() else {
        return;
    };
    let name = command.name.clone();
    let invocation = expand::Invocation {
        file: invocation.file,
        marked: invocation.marked,
        folder: invocation.folder,
        other_folder: invocation.other_folder,
        other_file: invocation.other_file,
    };
    let plan = match expand::plan(&command, &invocation, &search()) {
        Ok(plan) => plan,
        Err(error) => return refuse(&sender, &name, &UserCommandError::Refused(error)),
    };
    let exit_sender = sender.clone();
    let exit_name = name.clone();
    let hidden = plan.console == model::Console::Hide;
    let on_exit = Box::new(move |code| {
        let _ = exit_sender.send(UserCommandNews::Exited {
            name: exit_name,
            code,
            hidden,
        });
    });
    if let Err(error) = launch(plan, on_exit) {
        refuse(&sender, &name, &error);
    }
}

/// Finds the command called `name` in the loaded list.
fn find_command(
    name: &str,
    commands: &Mutex<Vec<UserCommand>>,
) -> Result<UserCommand, UserCommandError> {
    lock(commands)
        .iter()
        .find(|command| command.name == name)
        .cloned()
        .ok_or(UserCommandError::NoCommand)
}

/// Starts the expanded command; `on_exit` gets its exit code later.
fn launch(
    plan: expand::Plan,
    on_exit: Box<dyn FnOnce(Option<i32>) + Send>,
) -> Result<(), UserCommandError> {
    start::start(plan, on_exit).map_err(UserCommandError::Start)
}

/// Sends the localized refusal for `name` to the window.
fn refuse(sender: &UserCommandSender, name: &str, error: &UserCommandError) {
    let _ = sender.send(UserCommandNews::Refused {
        name: name.to_owned(),
        message: error.message(name),
    });
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
