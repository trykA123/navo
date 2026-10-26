use std::rc::Rc;

use rusty_core::Location;

use super::FerriteWindow;

type Open = Rc<dyn Fn(&Location) -> Result<(), String>>;

pub struct TerminalHooks {
    pub open: Open,
}

impl TerminalHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            open: Rc::new(|_| Err(rusty_l10n::tr("terminal-not-configured"))),
        }
    }
}

impl FerriteWindow {
    pub(super) fn open_terminal(&mut self) {
        let state = self.browser.state();
        let pane = state.focused_pane();
        let folder = state.active_tab().location();
        let result = if folder.is_local() {
            (self.terminal.open)(folder)
        } else {
            Err(rusty_l10n::tr("terminal-local-only"))
        };
        if let Err(error) = result {
            self.report_pane_error(pane, error.into());
        }
    }
}
