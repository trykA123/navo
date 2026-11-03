use crate::settings::{Filter, GitPage};
use crate::window_view::NavoWindow;

impl NavoWindow {
    /// The find words plus the rows the current values leave out.
    pub(super) fn settings_filter(&self) -> Filter {
        self.settings
            .filter
            .clone()
            .hiding(self.values.hidden_row())
    }

    pub(super) fn git_page(&self) -> GitPage {
        let Some(hooks) = self.git_badges.hooks() else {
            return GitPage::default();
        };
        let command = self.values.git_tool_command();
        let problem = (self.values.is_git_other() && !command.trim().is_empty())
            .then(|| hooks.check_command(command).err())
            .flatten();
        GitPage::new(hooks.tools(), problem)
    }

    pub(super) fn refresh_git_tools(&self) {
        if let Some(hooks) = self.git_badges.hooks() {
            hooks.refresh_tools();
        }
    }
}
