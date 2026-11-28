use std::sync::Arc;

use crate::git::GitHooks;
use crate::settings::{Filter, GitPage, Values};
use crate::window_view::NavoWindow;

/// Builds the Git settings page from the current hooks and values.
pub(super) fn git_page_for(hooks: Option<&Arc<dyn GitHooks>>, values: &Values) -> GitPage {
    let Some(hooks) = hooks else {
        return GitPage::default();
    };
    let command = values.git_tool_command();
    let problem = (values.is_git_other() && !command.trim().is_empty())
        .then(|| hooks.check_command(command).err())
        .flatten();
    GitPage::new(hooks.tools(), problem)
}

impl NavoWindow {
    /// The find words plus the rows the current values leave out.
    pub(super) fn settings_filter(&self) -> Filter {
        self.settings
            .filter
            .clone()
            .hiding(self.values.hidden_row())
            .folding(self.settings.folds)
    }

    pub(super) fn refresh_git_tools(&self) {
        if let Some(hooks) = self.git_badges.hooks() {
            hooks.refresh_tools();
        }
    }
}
