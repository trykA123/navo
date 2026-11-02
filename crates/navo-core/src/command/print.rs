use std::fmt;

use super::{Command, CursorMove, TabCommand};

enum Shown<'a> {
    Str(&'a str),
    Int(u64),
    Signed(i8),
    Owned(String),
    Word(&'static str),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id().as_str())?;
        let fields = fields(self);
        if fields.is_empty() {
            return Ok(());
        }
        f.write_str(" { ")?;
        for (i, (name, value)) in fields.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{name}: ")?;
            write_value(f, value)?;
        }
        f.write_str(" }")
    }
}

fn fields(command: &Command) -> Vec<(&'static str, Shown<'_>)> {
    match command {
        Command::PaneOpenTab { location } | Command::WindowOpen { location } => location
            .iter()
            .map(|l| ("location", Shown::Str(l.as_str())))
            .collect(),
        Command::PaneCloseTab { tab } => tab
            .iter()
            .map(|t| ("tab", Shown::Int(u64::from(*t))))
            .collect(),
        Command::PaneNavigate { location } => vec![("location", Shown::Str(location.as_str()))],
        Command::PaneGoDrive { root } => vec![("root", Shown::Str(root.as_str()))],
        Command::PaneCursorMove { to } => vec![("to", cursor_value(*to))],
        Command::SelectionExtend { to, mode } => vec![
            ("to", cursor_value(*to)),
            ("mode", Shown::Word(mode.keyword())),
        ],
        Command::SelectionExtension { mark } => {
            vec![("mark", Shown::Word(flag(*mark)))]
        }
        Command::SelectionRange { to } | Command::PaneScroll { to } => {
            vec![("to", Shown::Int(*to))]
        }
        Command::PaneActivateTab { tab } => vec![("tab", Shown::Int(u64::from(*tab)))],
        Command::WindowClose { window } => {
            window.iter().map(|w| ("window", Shown::Int(*w))).collect()
        }
        Command::WindowFocus { window } => vec![("window", Shown::Int(*window))],
        Command::SelectionAddPattern { pattern, folders }
        | Command::SelectionRemovePattern { pattern, folders } => {
            let mut shown = vec![("pattern", Shown::Str(pattern))];
            if *folders {
                shown.push(("folders", Shown::Word("true")));
            }
            shown
        }
        Command::WorkspaceSwitch { .. }
        | Command::WorkspaceNew { .. }
        | Command::WorkspaceRename { .. }
        | Command::WorkspaceMove { .. }
        | Command::WorkspaceDelete { .. }
        | Command::WorkspaceRestore
        | Command::WorkspaceSave
        | Command::WorkspaceDiscard { .. } => workspace_fields(command),
        _ => layout_fields(command),
    }
}

fn layout_fields(command: &Command) -> Vec<(&'static str, Shown<'_>)> {
    match command {
        Command::LayoutSplit { direction }
        | Command::LayoutFocus { direction }
        | Command::LayoutSwapToward { direction }
        | Command::LayoutResize { direction } => {
            vec![("direction", Shown::Word(direction.keyword()))]
        }
        Command::LayoutPreset { tiles } => vec![("tiles", Shown::Owned(tiles.print()))],
        Command::LayoutSwap { a, b } => {
            vec![("a", Shown::Int(*a as u64)), ("b", Shown::Int(*b as u64))]
        }
        Command::LayoutRatio { split, ratio } => vec![
            (
                "split",
                if split.0.is_empty() {
                    Shown::Word("root")
                } else {
                    Shown::Owned(split.to_string())
                },
            ),
            ("ratio", Shown::Int(u64::from(*ratio))),
        ],
        Command::LayoutFocusPane { n } => vec![("n", Shown::Int(*n as u64))],
        Command::Tab(command) => tab_fields(command),
        _ => Vec::new(),
    }
}

fn tab_fields(command: &TabCommand) -> Vec<(&'static str, Shown<'_>)> {
    let rest = match command {
        TabCommand::Pin { pinned, .. } => Some(("pinned", Shown::Word(flag(*pinned)))),
        TabCommand::Rename { label, .. } => Some(("label", Shown::Str(label))),
        TabCommand::MoveToWorkspace { n, .. } | TabCommand::MoveToPane { n, .. } => {
            Some(("n", Shown::Int(*n as u64)))
        }
        _ => None,
    };
    command
        .tab()
        .iter()
        .map(|tab| ("tab", Shown::Int(tab.get())))
        .chain(rest)
        .collect()
}

fn flag(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn workspace_fields(command: &Command) -> Vec<(&'static str, Shown<'_>)> {
    match command {
        Command::WorkspaceSwitch { n }
        | Command::WorkspaceDelete { n }
        | Command::WorkspaceDiscard { n } => vec![("n", Shown::Int(*n as u64))],
        Command::WorkspaceNew { name } => {
            name.iter().map(|name| ("name", Shown::Str(name))).collect()
        }
        Command::WorkspaceRename { n, name } => {
            vec![("n", Shown::Int(*n as u64)), ("name", Shown::Str(name))]
        }
        Command::WorkspaceMove { n, by } => {
            vec![("n", Shown::Int(*n as u64)), ("by", Shown::Signed(*by))]
        }
        _ => Vec::new(),
    }
}

fn cursor_value(to: CursorMove) -> Shown<'static> {
    match to {
        CursorMove::Row(row) => Shown::Int(row),
        other => Shown::Word(other.keyword().unwrap_or_default()),
    }
}

fn write_value(f: &mut fmt::Formatter<'_>, value: &Shown<'_>) -> fmt::Result {
    match value {
        Shown::Str(text) => write_string(f, text),
        Shown::Owned(text) => write_string(f, text),
        Shown::Signed(n) => write!(f, "{n}"),
        Shown::Int(n) => write!(f, "{n}"),
        Shown::Word(word) => f.write_str(word),
    }
}

pub(crate) fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    let _ = write_string(&mut out, text);
    out
}

fn write_string(f: &mut impl fmt::Write, text: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c if c.is_control() => write!(f, "\\u{{{:x}}}", u32::from(c))?,
            c => f.write_char(c)?,
        }
    }
    f.write_char('"')
}
