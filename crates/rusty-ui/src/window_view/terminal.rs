use std::sync::Arc;

use rusty_core::{Location, PaneId};

use super::FerriteWindow;
use crate::prelude::*;

type Open = Arc<dyn Fn(&Location) -> Result<(), String> + Send + Sync>;

pub struct TerminalHooks {
    pub open: Open,
}

impl TerminalHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            open: Arc::new(|_| Err(rusty_l10n::tr("terminal-not-configured"))),
        }
    }
}

impl FerriteWindow {
    pub(super) fn open_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.browser.state();
        let pane = state.focused_pane();
        let folder = state.active_tab().location().clone();
        if !folder.is_local() {
            let error = rusty_l10n::tr("terminal-local-only");
            return self.report_pane_error(pane, error.into());
        }
        let open = self.terminal.open.clone();
        self.run_off_thread(
            Box::new(move || {
                let opened = open(&folder);
                Box::new(move |view, _, _| view.terminal_started(pane, opened))
            }),
            window,
            cx,
        );
    }

    fn terminal_started(&mut self, pane: PaneId, opened: Result<(), String>) {
        if let Err(error) = opened {
            self.report_pane_error(pane, error.into());
        }
    }
}
