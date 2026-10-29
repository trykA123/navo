use std::process::Command;
use std::sync::{Arc, mpsc};

use rusty_core::Location;
use rusty_ui::browser::Launcher;

pub fn launcher() -> Launcher {
    queued(|mut command| {
        let _ = command.spawn();
    })
}

fn queued(start: impl Fn(Command) + Send + 'static) -> Launcher {
    let (sender, commands) = mpsc::channel::<Command>();
    let worker = std::thread::Builder::new()
        .name("navo-launch".to_owned())
        .spawn(move || {
            for command in commands {
                start(command);
            }
        });
    if let Err(error) = worker {
        tracing::warn!(%error, "could not start the launch worker");
    }
    Arc::new(move |location: &Location| {
        if let Some(command) = command_for(location) {
            let _ = sender.send(command);
        }
    })
}

fn opener() -> &'static str {
    if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    }
}

fn command_for(location: &Location) -> Option<Command> {
    if !location.is_local() {
        return None;
    }
    let mut command = Command::new(opener());
    command.arg(location.as_str());
    Some(command)
}

#[cfg(test)]
mod tests;
