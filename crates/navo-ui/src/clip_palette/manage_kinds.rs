//! The kinds of the library view, and the search that fills them.

use navo_core::Location;
use navo_l10n::tr;

use super::{ClipKind, ClipMode, ClipRow, ClipboardHooks};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    History,
    Snippets,
    Commands,
    Paths,
    Secrets,
    Notes,
}

pub(super) const KINDS: [Kind; 6] = [
    Kind::History,
    Kind::Snippets,
    Kind::Commands,
    Kind::Paths,
    Kind::Secrets,
    Kind::Notes,
];

impl Kind {
    /// The hook mode that lists this kind. Commands are saved text with no kind of their own,
    /// so they have none yet.
    fn mode(self) -> Option<ClipMode> {
        match self {
            Self::History => Some(ClipMode::Recent),
            Self::Snippets | Self::Paths => Some(ClipMode::Saved),
            Self::Secrets => Some(ClipMode::Secrets),
            Self::Notes => Some(ClipMode::Notes),
            Self::Commands => None,
        }
    }

    fn takes(self, row: &ClipRow) -> bool {
        match self {
            Self::History | Self::Secrets => true,
            Self::Snippets => matches!(row.kind, ClipKind::Text | ClipKind::Link),
            Self::Paths => matches!(row.kind, ClipKind::Path | ClipKind::Files),
            Self::Notes => row.kind == ClipKind::Note,
            Self::Commands => false,
        }
    }

    pub fn label(self) -> String {
        tr(match self {
            Self::History => "clip-manage-history",
            Self::Snippets => "clip-manage-snippets",
            Self::Commands => "clip-manage-commands",
            Self::Paths => "clip-manage-paths",
            Self::Secrets => "clip-manage-secrets",
            Self::Notes => "clip-manage-notes",
        })
    }

    /// The palette mode whose empty-state text fits this kind.
    pub fn empty_mode(self) -> ClipMode {
        match self {
            Self::History => ClipMode::Recent,
            Self::Snippets | Self::Commands | Self::Paths => ClipMode::Saved,
            Self::Secrets => ClipMode::Secrets,
            Self::Notes => ClipMode::Notes,
        }
    }
}

/// What one refresh asks: the search, and the kinds the hooks can list.
pub(super) struct Ask {
    pub query: String,
    pub folder: Option<Location>,
    pub kind: Kind,
    pub kinds: Vec<Kind>,
}

/// The rows of the current kind and a count for each listed kind.
pub(super) struct Answer {
    pub rows: Vec<ClipRow>,
    pub counts: Vec<(Kind, usize)>,
}

/// Searches each mode once and sorts its rows into kinds. A secret row keeps no user name.
pub(super) fn gather(hooks: &dyn ClipboardHooks, ask: &Ask) -> Answer {
    let mut searched: Vec<(ClipMode, Vec<ClipRow>)> = Vec::new();
    let (mut rows, mut counts) = (Vec::new(), Vec::new());
    for kind in ask.kinds.iter().copied() {
        let Some(mode) = kind.mode() else { continue };
        let at = searched.iter().position(|(seen, _)| *seen == mode);
        let at = at.unwrap_or_else(|| {
            let found = hooks.search(&ask.query, mode, ask.folder.as_ref()).rows;
            searched.push((mode, found));
            searched.len() - 1
        });
        let mine = searched[at].1.iter().filter(|row| kind.takes(row));
        counts.push((kind, mine.clone().count()));
        if kind == ask.kind {
            rows = mine.cloned().collect();
        }
    }
    for row in rows.iter_mut().filter(|row| row.kind == ClipKind::Secret) {
        row.detail.clear();
    }
    Answer { rows, counts }
}

/// The kinds the hooks offer a mode for, in the nav's order.
pub(super) fn offered(modes: &[ClipMode]) -> Vec<Kind> {
    KINDS
        .into_iter()
        .filter(|kind| kind.mode().is_some_and(|mode| modes.contains(&mode)))
        .collect()
}
