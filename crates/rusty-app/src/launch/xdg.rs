use std::process::Command;

use super::LaunchRequest;

pub(super) fn start(request: LaunchRequest) {
    let mut command = Command::new("xdg-open");
    command.arg(&request.file);
    if let Some(directory) = &request.directory {
        command.current_dir(directory);
    }
    if let Err(error) = command.spawn() {
        tracing::warn!(%error, file = %request.file, "could not start xdg-open");
    }
}
