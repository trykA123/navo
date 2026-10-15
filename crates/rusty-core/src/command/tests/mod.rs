use crate::{Command, CommandId, CursorMove, Direction, Location, ParseError, ParseErrorKind};

mod errors;
mod ids;
mod parsing;
mod props;
mod round_trip;
mod snapshots;

fn location(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn parse(text: &str) -> Result<Command, ParseError> {
    Command::parse(text)
}

fn error(offset: usize, kind: ParseErrorKind) -> Result<Command, ParseError> {
    Err(ParseError { offset, kind })
}

fn printed(text: &str) -> Result<String, ParseError> {
    parse(text).map(|c| c.to_string())
}

const CANONICAL_EXAMPLES: &[&str] = &[
    r#"pane.open-tab { location: "C:\\Users\\admin" }"#,
    "pane.close-tab { tab: 2 }",
    r#"pane.navigate { location: "/home/admin" }"#,
    r#"pane.go-drive { root: "D:\\" }"#,
    "pane.back",
    "pane.forward",
    "pane.refresh",
    "pane.cursor-move { to: page-down }",
    "pane.activate-tab { tab: 1 }",
    "pane.scroll { to: 120 }",
    "selection.toggle",
    "selection.toggle-down",
    "selection.range { to: 7 }",
    r#"selection.add-pattern { pattern: "*.rs" }"#,
    r#"selection.remove-pattern { pattern: "?.txt" }"#,
    "selection.invert",
    "selection.all",
    "selection.clear",
    "layout.split { direction: right }",
    "layout.close",
    "layout.focus { direction: up }",
    r#"window.open { location: "D:\\Work" }"#,
    "window.close { window: 3 }",
    "window.focus { window: 1 }",
    "app.save-session",
];

fn id_part(text: &str) -> &str {
    text.split(' ').next().unwrap_or(text)
}
