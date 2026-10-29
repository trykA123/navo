use std::process::Command;

use super::LaunchRequest;

#[expect(
    clippy::needless_pass_by_value,
    reason = "the launcher worker calls start with an owned request"
)]
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
