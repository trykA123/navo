use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    PaneOpenTab,
    PaneCloseTab,
    PaneNavigate,
    PaneGoDrive,
    PaneBack,
    PaneForward,
    PaneRefresh,
    PaneCursorMove,
    PaneActivateTab,
    PaneScroll,
    SelectionToggle,
    SelectionToggleDown,
    SelectionRange,
    SelectionExtend,
    SelectionExtension,
    SelectionRestore,
    SelectionAddPattern,
    SelectionRemovePattern,
    SelectionInvert,
    SelectionAll,
    SelectionClear,
    LayoutSplit,
    LayoutClose,
    LayoutFocus,
    WindowOpen,
    WindowClose,
    WindowFocus,
    WorkspaceSwitch,
    WorkspaceNew,
    WorkspaceRename,
    WorkspaceMove,
    WorkspaceDelete,
    WorkspaceRestore,
    WorkspaceSave,
    WorkspaceDiscard,
    AppSaveSession,
}

impl CommandId {
    pub const ALL: &'static [Self] = &[
        Self::PaneOpenTab,
        Self::PaneCloseTab,
        Self::PaneNavigate,
        Self::PaneGoDrive,
        Self::PaneBack,
        Self::PaneForward,
        Self::PaneRefresh,
        Self::PaneCursorMove,
        Self::PaneActivateTab,
        Self::PaneScroll,
        Self::SelectionToggle,
        Self::SelectionToggleDown,
        Self::SelectionRange,
        Self::SelectionExtend,
        Self::SelectionExtension,
        Self::SelectionRestore,
        Self::SelectionAddPattern,
        Self::SelectionRemovePattern,
        Self::SelectionInvert,
        Self::SelectionAll,
        Self::SelectionClear,
        Self::LayoutSplit,
        Self::LayoutClose,
        Self::LayoutFocus,
        Self::WindowOpen,
        Self::WindowClose,
        Self::WindowFocus,
        Self::WorkspaceSwitch,
        Self::WorkspaceNew,
        Self::WorkspaceRename,
        Self::WorkspaceMove,
        Self::WorkspaceDelete,
        Self::WorkspaceRestore,
        Self::WorkspaceSave,
        Self::WorkspaceDiscard,
        Self::AppSaveSession,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PaneOpenTab => "pane.open-tab",
            Self::PaneCloseTab => "pane.close-tab",
            Self::PaneNavigate => "pane.navigate",
            Self::PaneGoDrive => "pane.go-drive",
            Self::PaneBack => "pane.back",
            Self::PaneForward => "pane.forward",
            Self::PaneRefresh => "pane.refresh",
            Self::PaneCursorMove => "pane.cursor-move",
            Self::PaneActivateTab => "pane.activate-tab",
            Self::PaneScroll => "pane.scroll",
            Self::SelectionToggle => "selection.toggle",
            Self::SelectionToggleDown => "selection.toggle-down",
            Self::SelectionRange => "selection.range",
            Self::SelectionExtend => "selection.extend",
            Self::SelectionExtension => "selection.extension",
            Self::SelectionRestore => "selection.restore",
            Self::SelectionAddPattern => "selection.add-pattern",
            Self::SelectionRemovePattern => "selection.remove-pattern",
            Self::SelectionInvert => "selection.invert",
            Self::SelectionAll => "selection.all",
            Self::SelectionClear => "selection.clear",
            Self::LayoutSplit => "layout.split",
            Self::LayoutClose => "layout.close",
            Self::LayoutFocus => "layout.focus",
            Self::WindowOpen => "window.open",
            Self::WindowClose => "window.close",
            Self::WindowFocus => "window.focus",
            Self::WorkspaceSwitch => "workspace.switch",
            Self::WorkspaceNew => "workspace.new",
            Self::WorkspaceRename => "workspace.rename",
            Self::WorkspaceMove => "workspace.move",
            Self::WorkspaceDelete => "workspace.delete",
            Self::WorkspaceRestore => "workspace.restore",
            Self::WorkspaceSave => "workspace.save",
            Self::WorkspaceDiscard => "workspace.discard",
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
