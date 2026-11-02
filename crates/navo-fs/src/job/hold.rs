use super::Progress;
use crate::CancelToken;

pub(super) fn hold(
    cancel: &CancelToken,
    progress: &mut Progress,
    report: &mut dyn FnMut(&Progress),
) {
    if !cancel.is_paused() || cancel.is_cancelled() {
        return;
    }
    progress.paused = true;
    report(progress);
    cancel.wait_while_paused();
    progress.paused = false;
    report(progress);
}
