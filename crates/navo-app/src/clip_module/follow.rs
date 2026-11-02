use std::rc::Rc;
use std::sync::Arc;

use navo_ui::settings::Values;
use navo_ui::window_view::ResidentHooks;

use super::ClipModule;

/// Resident hooks that also tell the clipboard module about every settings change.
/// `apply` runs on the window thread only.
struct Follow {
    module: Arc<ClipModule>,
    inner: Option<Rc<dyn ResidentHooks>>,
}

pub(crate) fn follow(
    module: &Arc<ClipModule>,
    inner: Option<Rc<dyn ResidentHooks>>,
) -> Rc<dyn ResidentHooks> {
    Rc::new(Follow {
        module: module.clone(),
        inner,
    })
}

impl ResidentHooks for Follow {
    fn apply(&self, values: &Values) {
        if let Some(inner) = &self.inner {
            inner.apply(values);
        }
        self.module.apply(values);
    }

    fn key_taken(&self) -> bool {
        self.inner.as_ref().is_some_and(|inner| inner.key_taken())
    }

    fn clipboard_key_taken(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|inner| inner.clipboard_key_taken())
    }

    fn has_tray(&self) -> bool {
        self.inner.as_ref().is_some_and(|inner| inner.has_tray())
    }
}
