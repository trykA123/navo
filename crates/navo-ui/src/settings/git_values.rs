use super::fields::Field;
use super::git_fields::FETCH_MINUTES;
use super::git_page::OTHER;
use super::values::{Value, Values};

pub(super) const DEFAULT_FETCH_MINUTES: i64 = 15;

impl Values {
    pub fn git_badges(&self) -> bool {
        self.flag(Field::GitBadges)
    }

    /// The stored `[git] tool`: `auto`, `none`, `git-cli`, `custom` or a tool id.
    pub fn git_tool(&self) -> &str {
        self.text_of(Field::GitTool)
    }

    pub fn git_tool_command(&self) -> &str {
        self.text_of(Field::GitToolCommand)
    }

    pub fn is_git_other(&self) -> bool {
        self.git_tool() == OTHER
    }

    /// The row Settings leaves out: the command of Other, while another tool is chosen.
    pub fn hidden_row(&self) -> Option<Field> {
        (!self.is_git_other()).then_some(Field::GitToolCommand)
    }

    pub fn git_engine(&self) -> &'static str {
        self.toml_of(Field::GitEngine)
    }

    pub fn git_fetch(&self) -> bool {
        self.flag(Field::GitFetch)
    }

    /// Minutes between background fetches, clamped to 5 to 120.
    pub fn git_fetch_minutes(&self) -> i64 {
        match self.get(Field::GitFetchMinutes) {
            Value::Number(minutes) => (*minutes).clamp(FETCH_MINUTES.0, FETCH_MINUTES.1),
            _ => DEFAULT_FETCH_MINUTES,
        }
    }

    pub fn git_network(&self) -> bool {
        self.flag(Field::GitNetwork)
    }

    /// False for a row whose controlling setting is off.
    pub fn is_enabled(&self, field: Field) -> bool {
        field != Field::GitFetchMinutes || self.git_fetch()
    }
}
