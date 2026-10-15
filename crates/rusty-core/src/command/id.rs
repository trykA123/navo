use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    PaneOpenTab,
    PaneCloseTab,
    PaneNavigate,
    PaneBack,
    PaneForward,
    PaneRefresh,
    PaneCursorMove,
    SelectionToggle,
    SelectionToggleDown,
    SelectionRange,
    SelectionAddPattern,
    SelectionRemovePattern,
    SelectionInvert,
    SelectionAll,
    SelectionClear,
    LayoutSplit,
    LayoutClose,
    LayoutFocus,
    AppSaveSession,
}

impl CommandId {
    pub const ALL: &'static [Self] = &[
        Self::PaneOpenTab,
        Self::PaneCloseTab,
        Self::PaneNavigate,
        Self::PaneBack,
        Self::PaneForward,
        Self::PaneRefresh,
        Self::PaneCursorMove,
        Self::SelectionToggle,
        Self::SelectionToggleDown,
        Self::SelectionRange,
        Self::SelectionAddPattern,
        Self::SelectionRemovePattern,
        Self::SelectionInvert,
        Self::SelectionAll,
        Self::SelectionClear,
        Self::LayoutSplit,
        Self::LayoutClose,
        Self::LayoutFocus,
        Self::AppSaveSession,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PaneOpenTab => "pane.open-tab",
            Self::PaneCloseTab => "pane.close-tab",
            Self::PaneNavigate => "pane.navigate",
            Self::PaneBack => "pane.back",
            Self::PaneForward => "pane.forward",
            Self::PaneRefresh => "pane.refresh",
            Self::PaneCursorMove => "pane.cursor-move",
            Self::SelectionToggle => "selection.toggle",
            Self::SelectionToggleDown => "selection.toggle-down",
            Self::SelectionRange => "selection.range",
            Self::SelectionAddPattern => "selection.add-pattern",
            Self::SelectionRemovePattern => "selection.remove-pattern",
            Self::SelectionInvert => "selection.invert",
            Self::SelectionAll => "selection.all",
            Self::SelectionClear => "selection.clear",
            Self::LayoutSplit => "layout.split",
            Self::LayoutClose => "layout.close",
            Self::LayoutFocus => "layout.focus",
            Self::AppSaveSession => "app.save-session",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|id| id.as_str() == name)
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
