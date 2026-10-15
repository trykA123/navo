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
    "pane.back",
    "pane.forward",
    "pane.cursor-move { to: page-down }",
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
    "app.save-session",
];

fn id_part(text: &str) -> &str {
    text.split(' ').next().unwrap_or(text)
}
