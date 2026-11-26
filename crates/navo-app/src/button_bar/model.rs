//! The button bar file: `buttonbar.toml` into parsed items and line errors.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use navo_icons::spec::{IconSpec, parse_table, parse_text};
use serde::Deserialize;
use toml::Spanned;

use crate::editor::Search;
use crate::user_commands::UserCommand;
use crate::user_commands::model::{Console, Raw as RawCommand, line_at, read_command};

const MAX_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 200;
const MAX_MENU_CHILDREN: usize = 50;
const MAX_NAME_CHARS: usize = 64;

/// One problem in `buttonbar.toml` with its 1-based line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LineError {
    /// The 1-based line in `buttonbar.toml`.
    pub line: usize,
    /// The problem description.
    pub message: String,
}

impl fmt::Display for LineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for LineError {}

/// One parsed button bar entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedItem {
    /// The button or menu label (empty for separators).
    pub name: String,
    /// Optional parsed icon specification.
    pub icon: Option<IconSpec>,
    /// What the item does.
    pub kind: ParsedKind,
    /// The 1-based line where the item starts in `buttonbar.toml`.
    pub line: usize,
}

/// The action or structure of one button bar entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParsedKind {
    /// A built-in Navo command id.
    Navo(String),
    /// The name of a command from `user-commands.toml`.
    User(String),
    /// An inline program validated by the user-commands model.
    Program(UserCommand),
    /// A visual divider between buttons.
    Separator,
    /// A dropdown menu of child items.
    Menu(Vec<ParsedItem>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    item: Vec<Spanned<RawItem>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItem {
    #[serde(default)]
    name: Option<Spanned<String>>,
    #[serde(default)]
    icon: Option<Spanned<toml::Value>>,
    #[serde(default)]
    navo: Option<Spanned<String>>,
    #[serde(default)]
    user: Option<Spanned<String>>,
    #[serde(default)]
    program: Option<Spanned<String>>,
    #[serde(default)]
    args: Option<Spanned<String>>,
    #[serde(default, rename = "start-in")]
    start_in: Option<Spanned<String>>,
    #[serde(default)]
    console: Option<Console>,
    #[serde(default)]
    key: Option<Spanned<String>>,
    #[serde(default)]
    separator: Option<Spanned<bool>>,
    #[serde(default)]
    menu: Option<Vec<Spanned<RawItem>>>,
}

/// Reads `buttonbar.toml` and returns parsed items and warnings, or every line error found.
pub(crate) fn parse(
    text: &str,
    search: Option<&Search>,
) -> Result<(Vec<ParsedItem>, Vec<String>), Vec<LineError>> {
    if text.len() > MAX_BYTES {
        return Err(vec![LineError {
            line: 1,
            message: "buttonbar.toml is larger than 256 KiB".to_owned(),
        }]);
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let File { item } = match toml::from_str(text) {
        Ok(file) => file,
        Err(error) => return Err(vec![syntax_error(text, &error)]),
    };
    let mut ctx = ParseCtx {
        text,
        search,
        total_items: 0,
        errors: Vec::new(),
    };
    let mut items = Vec::with_capacity(item.len());
    for entry in item {
        if let Some(parsed) = ctx.read_item(entry, false) {
            items.push(parsed);
        }
    }
    ctx.errors.sort_by_key(|error| error.line);
    if ctx.errors.is_empty() {
        Ok((items, Vec::new()))
    } else {
        Err(ctx.errors)
    }
}

struct ParseCtx<'a> {
    text: &'a str,
    search: Option<&'a Search>,
    total_items: usize,
    errors: Vec<LineError>,
}

impl ParseCtx<'_> {
    fn push(&mut self, line: usize, message: impl Into<String>) {
        self.errors.push(LineError {
            line,
            message: message.into(),
        });
    }

    fn count_item(&mut self, line: usize) -> bool {
        self.total_items += 1;
        if self.total_items == MAX_ITEMS + 1 {
            self.push(line, "buttonbar.toml can have at most 200 items");
        }
        self.total_items <= MAX_ITEMS
    }

    fn read_item(&mut self, entry: Spanned<RawItem>, in_menu: bool) -> Option<ParsedItem> {
        let span = entry.span();
        let line = line_at(self.text, span.start);
        if !self.count_item(line) {
            return None;
        }
        let raw = entry.into_inner();
        let start_errors = self.errors.len();
        self.check_common(&raw, line);
        let actions = action_count(&raw);
        if actions == 0 {
            self.push(
                line,
                "item needs one of navo, user, program, separator, or menu",
            );
            return None;
        }
        if actions > 1 {
            self.push(
                line,
                "item must have only one of navo, user, program, separator, or menu",
            );
            return None;
        }
        if is_separator(&raw) {
            return self.read_separator(&raw, line, start_errors);
        }
        let icon = raw.icon.as_ref().and_then(|value| self.read_icon(value));
        let name = self.read_name(raw.name.as_ref(), line, raw.program.is_none());
        let kind = self.read_action(span, raw, line, in_menu)?;
        if self.errors.len() == start_errors {
            Some(ParsedItem {
                name: name.unwrap_or_default(),
                icon,
                kind,
                line,
            })
        } else {
            None
        }
    }

    fn check_common(&mut self, raw: &RawItem, line: usize) {
        if let Some(key) = &raw.key {
            self.push(
                line_at(self.text, key.span().start),
                "a button has no key; bind a user command instead",
            );
        }
        if let Some(sep) = &raw.separator
            && !*sep.get_ref()
        {
            self.push(
                line_at(self.text, sep.span().start),
                "separator must be true",
            );
        }
        if raw.program.is_none() {
            self.check_program_only_fields(raw, line);
        }
    }

    fn check_program_only_fields(&mut self, raw: &RawItem, line: usize) {
        if let Some(args) = &raw.args {
            self.push(
                line_at(self.text, args.span().start),
                "args can only be used with program",
            );
        }
        if let Some(start_in) = &raw.start_in {
            self.push(
                line_at(self.text, start_in.span().start),
                "start-in can only be used with program",
            );
        }
        if raw.console.is_some() {
            self.push(line, "console can only be used with program");
        }
    }

    fn read_separator(
        &mut self,
        raw: &RawItem,
        line: usize,
        start_errors: usize,
    ) -> Option<ParsedItem> {
        if let Some(name) = &raw.name {
            self.push(
                line_at(self.text, name.span().start),
                "a separator cannot have a name",
            );
        }
        if let Some(icon) = &raw.icon {
            self.push(
                line_at(self.text, icon.span().start),
                "a separator cannot have an icon",
            );
        }
        if self.errors.len() == start_errors {
            Some(ParsedItem {
                name: String::new(),
                icon: None,
                kind: ParsedKind::Separator,
                line,
            })
        } else {
            None
        }
    }

    fn read_name(
        &mut self,
        name: Option<&Spanned<String>>,
        item_line: usize,
        check_empty: bool,
    ) -> Option<String> {
        let Some(name) = name else {
            self.push(item_line, "item is missing name");
            return None;
        };
        let line = line_at(self.text, name.span().start);
        let value = name.get_ref();
        if check_empty && value.trim().is_empty() {
            self.push(line, "item name cannot be empty");
            return None;
        }
        if value.chars().count() > MAX_NAME_CHARS {
            self.push(line, "item name must be at most 64 characters");
            return None;
        }
        Some(value.clone())
    }

    fn read_icon(&mut self, icon: &Spanned<toml::Value>) -> Option<IconSpec> {
        let line = line_at(self.text, icon.span().start);
        let parsed = match icon.get_ref() {
            toml::Value::String(text) => parse_text(text),
            toml::Value::Table(table) => parse_table(table),
            _ => {
                self.push(line, "icon must be text or a table");
                return None;
            }
        };
        match parsed {
            Ok(spec) => Some(spec),
            Err(error) => {
                self.push(line, error.0);
                None
            }
        }
    }

    fn read_action(
        &mut self,
        span: Range<usize>,
        raw: RawItem,
        line: usize,
        in_menu: bool,
    ) -> Option<ParsedKind> {
        if let Some(navo) = raw.navo {
            return self.read_navo(navo);
        }
        if let Some(user) = raw.user {
            return self.read_user(user);
        }
        if raw.program.is_some() {
            return self.read_program(span, raw);
        }
        if let Some(children) = raw.menu {
            return self.read_menu(children, line, in_menu);
        }
        None
    }

    fn read_navo(&mut self, navo: Spanned<String>) -> Option<ParsedKind> {
        let line = line_at(self.text, navo.span().start);
        let id = navo.into_inner();
        if !is_valid_navo_id(&id) {
            self.push(line, format!("invalid navo command id `{id}`"));
            return None;
        }
        Some(ParsedKind::Navo(id))
    }

    fn read_user(&mut self, user: Spanned<String>) -> Option<ParsedKind> {
        let line = line_at(self.text, user.span().start);
        let target = user.into_inner();
        if target.trim().is_empty() {
            self.push(line, "user command name cannot be empty");
            return None;
        }
        Some(ParsedKind::User(target))
    }

    fn read_program(&mut self, span: Range<usize>, raw: RawItem) -> Option<ParsedKind> {
        let (Some(name), Some(program)) = (raw.name, raw.program) else {
            return None;
        };
        let command = RawCommand {
            name,
            program,
            args: raw.args,
            start_in: raw.start_in,
            console: raw.console,
            key: None,
        };
        let mut names = HashMap::new();
        let (parsed, found) = read_command(
            self.text,
            Spanned::new(span, command),
            &mut names,
            self.search,
        );
        for error in found {
            self.push(error.line, error.message());
        }
        parsed.map(ParsedKind::Program)
    }

    fn read_menu(
        &mut self,
        children: Vec<Spanned<RawItem>>,
        line: usize,
        in_menu: bool,
    ) -> Option<ParsedKind> {
        if in_menu {
            self.push(line, "menus cannot be nested");
            return None;
        }
        if children.is_empty() {
            self.push(line, "menu must have at least one item");
            return None;
        }
        if children.len() > MAX_MENU_CHILDREN {
            self.push(line, "menu can have at most 50 items");
            return None;
        }
        let mut parsed = Vec::with_capacity(children.len());
        for child in children {
            if let Some(item) = self.read_item(child, true) {
                parsed.push(item);
            }
        }
        Some(ParsedKind::Menu(parsed))
    }
}

fn action_count(raw: &RawItem) -> usize {
    usize::from(raw.navo.is_some())
        + usize::from(raw.user.is_some())
        + usize::from(raw.program.is_some())
        + usize::from(is_separator(raw))
        + usize::from(raw.menu.is_some())
}

fn is_separator(raw: &RawItem) -> bool {
    raw.separator.as_ref().is_some_and(|flag| *flag.get_ref())
}

/// Checks `^[a-z][a-z_]*(\.[a-z_]+)+$`.
fn is_valid_navo_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    let mut first_chars = first.chars();
    let first_ok = first_chars.next().is_some_and(|ch| ch.is_ascii_lowercase())
        && first_chars.all(|ch| ch.is_ascii_lowercase() || ch == '_');
    if !first_ok {
        return false;
    }
    let mut saw_dot = false;
    for part in parts {
        saw_dot = true;
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_') {
            return false;
        }
    }
    saw_dot
}

fn syntax_error(text: &str, error: &toml::de::Error) -> LineError {
    let line = error.span().map_or(1, |span| line_at(text, span.start));
    LineError {
        line,
        message: error.message().to_owned(),
    }
}

#[cfg(test)]
mod tests;
