use std::fmt;

use super::{Command, CursorMove};

enum Shown<'a> {
    Str(&'a str),
    Int(u64),
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
        Command::PaneCursorMove { to } => vec![("to", cursor_value(*to))],
        Command::SelectionRange { to } | Command::PaneScroll { to } => {
            vec![("to", Shown::Int(*to))]
        }
        Command::PaneActivateTab { tab } => vec![("tab", Shown::Int(u64::from(*tab)))],
        Command::WindowClose { window } => {
            window.iter().map(|w| ("window", Shown::Int(*w))).collect()
        }
        Command::WindowFocus { window } => vec![("window", Shown::Int(*window))],
        Command::SelectionAddPattern { pattern } | Command::SelectionRemovePattern { pattern } => {
            vec![("pattern", Shown::Str(pattern))]
        }
        Command::LayoutSplit { direction } | Command::LayoutFocus { direction } => {
            vec![("direction", Shown::Word(direction.keyword()))]
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
