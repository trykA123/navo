use crate::{
    Command, CommandId, CursorMove, Direction, Location, ParseError, ParseErrorKind, SpanMode,
};

mod errors;
mod ids;
mod parsing;
mod props;
mod round_trip;
mod selection;
mod snapshots;
mod tab;
mod workspace;

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
    "selection.extend { to: down, mode: toggle }",
    "selection.extension { mark: true }",
    "selection.restore",
    r#"selection.add-pattern { pattern: "*.rs" }"#,
    r#"selection.remove-pattern { pattern: "?.txt" }"#,
    "selection.invert",
    "selection.all",
    "selection.clear",
    "layout.split { direction: right }",
    r#"layout.preset { tiles: "row(50, pane, pane)" }"#,
    "layout.swap { a: 1, b: 4 }",
    "layout.swap-toward { direction: left }",
    "layout.resize { direction: right }",
    "layout.ratio { split: root, ratio: 700 }",
    "layout.even",
    "layout.focus-pane { n: 4 }",
    "layout.close",
    "layout.focus { direction: up }",
    r#"window.open { location: "D:\\Work" }"#,
    "window.close { window: 3 }",
    "window.focus { window: 1 }",
    "workspace.switch { n: 2 }",
    r#"workspace.new { name: "Project" }"#,
    r#"workspace.rename { n: 1, name: "Work" }"#,
    "workspace.move { n: 2, by: -1 }",
    "workspace.delete { n: 2 }",
    "workspace.restore",
    "workspace.save",
    "workspace.discard { n: 1 }",
    "tab.pin { tab: 7, pinned: true }",
    r#"tab.rename { tab: 7, label: "Logs" }"#,
    "tab.duplicate { tab: 7 }",
    "tab.close { tab: 7 }",
    "tab.close-others { tab: 7 }",
    "tab.close-right { tab: 7 }",
    "tab.move-to-workspace { tab: 7, n: 2 }",
    "tab.move-to-new-workspace { tab: 7 }",
    "tab.move-to-pane { tab: 7, n: 3 }",
    "tab.move-to-new-pane { tab: 7 }",
    "tab.reopen",
    "app.save-session",
];

fn id_part(text: &str) -> &str {
    text.split(' ').next().unwrap_or(text)
}
