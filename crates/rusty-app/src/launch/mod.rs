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

fn command_for(location: &Location) -> Option<Command> {
    if !location.is_local() {
        return None;
    }
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let mut command = Command::new(program);
    command.arg(location.as_str());
    Some(command)
}

#[cfg(test)]
mod tests;
