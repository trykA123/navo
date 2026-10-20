use std::process::Command;
use std::sync::Arc;

use rusty_core::Location;
use rusty_ui::browser::Launcher;

pub fn launcher() -> Launcher {
    Arc::new(|location: &Location| {
        if let Some(mut command) = command_for(location) {
            let _ = command.spawn();
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
