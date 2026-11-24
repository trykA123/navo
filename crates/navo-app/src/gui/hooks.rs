//! The hooks that feed the window's start path, instance inbox and compare sides.

use std::path::PathBuf;
use std::sync::Arc;

use navo_ui::window_view::{OpenHooks, OpenInbox};

use crate::startup::path_location;

/// Builds the hooks the window receives for opened paths and instance requests.
pub(super) fn open_hooks(
    path: Option<PathBuf>,
    inbox: OpenInbox,
    splash: crate::splash::Splash,
    diff: Arc<crate::compare::diff::DiffBackend>,
) -> OpenHooks {
    OpenHooks {
        path: path.and_then(|path| path_location(&path)),
        inbox: Some(inbox),
        on_shown: splash.on_shown(),
        window_shown: crate::resident::window_shown(),
        resident: None,
        compare: Some(Arc::new(crate::compare::local::navo_pane_compare())),
        diff: Some(diff),
        user_commands: None,
    }
}
