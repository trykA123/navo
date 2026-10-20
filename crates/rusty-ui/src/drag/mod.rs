mod badge;
mod landing;

#[cfg(test)]
mod tests;

use rusty_core::{DropOp, PaneId};
use rusty_l10n::{t, tr};

use crate::SharedString;

pub use badge::DragBadge;
pub use landing::Landing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDrag {
    pub pane: PaneId,
    pub row: u64,
    pub name: SharedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hover {
    pub target_label: SharedString,
    pub op: Option<DropOp>,
    pub refusal: Option<SharedString>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragState {
    pub count: usize,
    pub first_name: SharedString,
    pub hover: Option<Hover>,
    pub keys_line: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Carry,
    Op(DropOp),
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lines {
    pub mark: Mark,
    pub head: String,
    pub keys: Option<String>,
}

fn count_of(state: &DragState) -> u64 {
    u64::try_from(state.count).unwrap_or(u64::MAX)
}

impl DragState {
    pub(crate) fn lines(&self) -> Lines {
        let count = count_of(self);
        let (mark, head) = match &self.hover {
            Some(Hover {
                refusal: Some(reason),
                ..
            }) => (Mark::Refused, reason.to_string()),
            Some(Hover {
                op: Some(op),
                target_label,
                ..
            }) => {
                let id = match op {
                    DropOp::Move => "drag-move",
                    DropOp::Copy => "drag-copy",
                    DropOp::Link => "drag-link",
                };
                (
                    Mark::Op(*op),
                    t!(id, count = count, target = target_label.to_string()),
                )
            }
            _ => (
                Mark::Carry,
                t!(
                    "drag-carry",
                    count = count,
                    name = self.first_name.to_string()
                ),
            ),
        };
        let keys = (self.keys_line && mark != Mark::Refused).then(|| tr("drag-keys"));
        Lines { mark, head, keys }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    SameFolder,
    IntoItself,
    ListChanged,
}

impl Refusal {
    #[must_use]
    pub fn text(self) -> SharedString {
        let id = match self {
            Self::SameFolder => "drag-refused-same-folder",
            Self::IntoItself => "drag-refused-into-itself",
            Self::ListChanged => "drag-refused-list-changed",
        };
        tr(id).into()
    }
}
