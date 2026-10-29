use std::sync::{Arc, mpsc};

use rusty_core::Location;
use rusty_ui::browser::Launcher;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::start;
#[cfg(not(windows))]
mod xdg;
#[cfg(not(windows))]
use xdg::start;

#[derive(Debug, PartialEq, Eq)]
struct LaunchRequest {
    file: String,
    directory: Option<String>,
}

pub fn launcher() -> Launcher {
    queued(start)
}

fn queued(start: impl Fn(LaunchRequest) + Send + 'static) -> Launcher {
    let (sender, requests) = mpsc::channel::<LaunchRequest>();
    let worker = std::thread::Builder::new()
        .name("navo-launch".to_owned())
        .spawn(move || {
            for request in requests {
                start(request);
            }
        });
    if let Err(error) = worker {
        tracing::warn!(%error, "could not start the launch worker");
    }
    Arc::new(move |location: &Location| {
        if let Some(request) = request_for(location) {
            let _ = sender.send(request);
        }
    })
}

fn request_for(location: &Location) -> Option<LaunchRequest> {
    if !location.is_local() {
        return None;
    }
    Some(LaunchRequest {
        file: location.as_str().to_owned(),
        directory: location.parent().map(|parent| parent.as_str().to_owned()),
    })
}

#[cfg(test)]
mod tests;
