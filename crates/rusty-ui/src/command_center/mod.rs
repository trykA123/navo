use crate::prelude::*;

mod commands_source;
mod file_sources;
mod query;
pub mod rank;
mod results;
mod selection;
mod source;
mod view;

#[cfg(test)]
mod tests;

pub use commands_source::{CommandRow, CommandsSnapshot, CommandsSource};
pub use file_sources::{
    Frecents, HereOptions, HereSource, HereWalks, RecentSource, TabsSnapshot, TabsSource,
};
pub use query::{Query, Scope, Term};
pub use results::{Heading, Results, Row};
pub use selection::Selection;
pub use source::{Cancel, Capped, Group, Hit, HitSink, Request, ResultSource, Scopes, Target};
pub use view::{
    Argument, CommandCenter, CommandCenterEvent, CommandCenterProps, HintKeys, INPUT_ROW, LEAVE,
};

pub use crate::commands::ArgKind;

gpui_kit::actions!(
    command_center,
    [
        Toggle,
        Up,
        Down,
        PageUp,
        PageDown,
        First,
        Last,
        NextGroup,
        PreviousGroup,
        Run,
        RunInNewTab,
        Back
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "command_center");
}
