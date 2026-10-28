use crate::prelude::*;

mod commands_source;
mod context_name;
mod file_sources;
pub mod flights;
pub mod index_source;
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
    Frecents, HereOptions, HereSource, HereWalks, PathListings, PathsSource, RecentSource,
    TabsSnapshot, TabsSource, completion,
};
pub use index_source::{IndexHooks, IndexMode, IndexOptions, IndexSource, Indexing};
pub use query::{Query, Scope, Term};
pub use results::{Heading, RESULTS_MEMORY, Results, Row};
pub use selection::Selection;
pub use source::{
    Cancel, Capped, Group, Hit, HitSink, Notes, Offline, Request, ResultSource, Scopes, Target,
};
pub use view::{
    Argument, CommandCenter, CommandCenterEvent, CommandCenterProps, HintKeys, INPUT_ROW,
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
        NextMode,
        PreviousMode,
        NextGroup,
        PreviousGroup,
        Run,
        RunInNewTab,
        Reveal,
        OtherPane,
        CopyPath,
        Back
    ]
);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "command_center");
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Names,
    Content,
    Commands,
    Collections,
}

impl Mode {
    pub(crate) const ALL: [Self; 4] = [
        Self::Names,
        Self::Content,
        Self::Commands,
        Self::Collections,
    ];

    pub(crate) fn cycle(self, is_previous: bool) -> Self {
        let at = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL[(at + if is_previous { 3 } else { 1 }) % Self::ALL.len()]
    }

    pub(crate) fn has_results(self) -> bool {
        matches!(self, Self::Names | Self::Commands)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Names => "command-center-mode-names",
            Self::Content => "command-center-mode-content",
            Self::Commands => "command-center-commands",
            Self::Collections => "command-center-mode-collections",
        }
    }
}
