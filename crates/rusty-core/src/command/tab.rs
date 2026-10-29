use super::CommandId;
use crate::TabId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabCommand {
    Pin { tab: Option<TabId>, pinned: bool },
    Rename { tab: Option<TabId>, label: String },
    Duplicate { tab: Option<TabId> },
    Close { tab: Option<TabId> },
    CloseOthers { tab: Option<TabId> },
    CloseRight { tab: Option<TabId> },
    MoveToWorkspace { tab: Option<TabId>, n: usize },
    MoveToNewWorkspace { tab: Option<TabId> },
    MoveToPane { tab: Option<TabId>, n: usize },
    MoveToNewPane { tab: Option<TabId> },
    Reopen,
}

impl TabCommand {
    #[must_use]
    pub fn id(&self) -> CommandId {
        match self {
            Self::Pin { .. } => CommandId::TabPin,
            Self::Rename { .. } => CommandId::TabRename,
            Self::Duplicate { .. } => CommandId::TabDuplicate,
            Self::Close { .. } => CommandId::TabClose,
            Self::CloseOthers { .. } => CommandId::TabCloseOthers,
            Self::CloseRight { .. } => CommandId::TabCloseRight,
            Self::MoveToWorkspace { .. } => CommandId::TabMoveToWorkspace,
            Self::MoveToNewWorkspace { .. } => CommandId::TabMoveToNewWorkspace,
            Self::MoveToPane { .. } => CommandId::TabMoveToPane,
            Self::MoveToNewPane { .. } => CommandId::TabMoveToNewPane,
            Self::Reopen => CommandId::TabReopen,
        }
    }

    #[must_use]
    pub fn tab(&self) -> Option<TabId> {
        match self {
            Self::Pin { tab, .. }
            | Self::Rename { tab, .. }
            | Self::Duplicate { tab }
            | Self::Close { tab }
            | Self::CloseOthers { tab }
            | Self::CloseRight { tab }
            | Self::MoveToWorkspace { tab, .. }
            | Self::MoveToNewWorkspace { tab }
            | Self::MoveToPane { tab, .. }
            | Self::MoveToNewPane { tab } => *tab,
            Self::Reopen => None,
        }
    }
}
