use std::sync::{Arc, mpsc};

use navo_core::Location;
use navo_ui::browser::Launcher;

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

/// Opens a shell target (a URL or a file) on the launch worker.
pub(crate) type Opener = Arc<dyn Fn(String) + Send + Sync>;

pub(crate) fn opener() -> Opener {
    open_with(start)
}

fn open_with(start: impl Fn(LaunchRequest) + Send + 'static) -> Opener {
    let sender = spawn_worker(start);
    Arc::new(move |file| {
        let _ = sender.send(LaunchRequest {
            file,
            directory: None,
        });
    })
}

fn queued(start: impl Fn(LaunchRequest) + Send + 'static) -> Launcher {
    let sender = spawn_worker(start);
    Arc::new(move |location: &Location| {
        if let Some(request) = request_for(location) {
            let _ = sender.send(request);
        }
    })
}

fn spawn_worker(start: impl Fn(LaunchRequest) + Send + 'static) -> mpsc::Sender<LaunchRequest> {
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
    sender
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
