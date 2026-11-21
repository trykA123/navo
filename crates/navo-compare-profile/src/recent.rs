//! Start rows: the Recent and Named lists of the start screen, built as plain
//! data from stored sessions (NS13).

use crate::session::{Session, SessionKind, SideKind, SideRef};

/// What the two sides of a start row point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// Both sides are Git revisions.
    GitRefs,
    /// Both sides are paths.
    Paths,
    /// One side is a path and the other a Git revision.
    Mixed,
}

/// One line of the start screen: a saved session reduced to display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartRow {
    /// Session id, used to reopen or delete it.
    pub id: String,
    /// User-given name, `None` for a recent session.
    pub name: Option<String>,
    /// Left side, as shown to the user.
    pub left: String,
    /// Right side, as shown to the user.
    pub right: String,
    /// Whether the sides are Git revisions, paths, or one of each.
    pub kind: RowKind,
    /// How long ago the session was opened, as display text.
    pub age: String,
}

/// The two start-screen lists: recent sessions and named sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartRows {
    /// Unnamed sessions, newest first, in the order given.
    pub recent: Vec<StartRow>,
    /// Named sessions, sorted by lower-cased name and then id.
    pub named: Vec<StartRow>,
}

/// Builds the two start-screen lists from `sessions` at Unix time `now`.
///
/// Named sessions go to `named`; the rest go to `recent` in the given order.
/// A `Texts` session and a name that is blank after trimming are left out of
/// `named`. Never fails.
pub fn start_rows(sessions: &[Session], now: i64) -> StartRows {
    let mut rows = StartRows::default();
    for session in sessions {
        if session.kind == SessionKind::Texts {
            continue;
        }
        let named = session
            .name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        let row = StartRow {
            id: session.id.clone(),
            name: if named { session.name.clone() } else { None },
            left: side_text(&session.left),
            right: side_text(&session.right),
            kind: row_kind(&session.left, &session.right),
            age: age_text(session.opened, now),
        };
        if named {
            rows.named.push(row);
        } else {
            rows.recent.push(row);
        }
    }
    rows.named.sort_by(|left, right| {
        name_key(left)
            .cmp(&name_key(right))
            .then_with(|| left.id.cmp(&right.id))
    });
    rows
}

/// Shows a side as `<path>`, `<path>@<rev>` for a Git side, or `<url>@<rev>` for a remote side.
///
/// The path is converted lossily, so invalid UTF-8 becomes a replacement
/// character. Never fails.
pub fn side_text(side: &SideRef) -> String {
    let path = side.path.to_string_lossy().into_owned();
    match (side.kind, side.rev.as_deref()) {
        (SideKind::Git, Some(rev)) => format!("{path}@{rev}"),
        (SideKind::Remote, Some(rev)) => {
            format!("{}@{rev}", side.url.as_deref().unwrap_or_default())
        }
        _ => path,
    }
}

/// Formats how long ago `opened` was, relative to Unix time `now`.
///
/// Ages under a minute, including a future `opened`, show `now`; then minutes,
/// hours, days, 30-day months and 365-day years, rounding down. Never fails.
pub fn age_text(opened: i64, now: i64) -> String {
    let seconds = now.saturating_sub(opened);
    match seconds {
        value if value < 60 => "now".to_string(),
        value if value < 3_600 => format!("{} min ago", value / 60),
        value if value < 86_400 => format!("{} h ago", value / 3_600),
        value if value < 30 * 86_400 => format!("{} d ago", value / 86_400),
        value if value < 365 * 86_400 => format!("{} mo ago", value / (30 * 86_400)),
        value => format!("{} y ago", value / (365 * 86_400)),
    }
}

/// Returns the row kind from the two side kinds.
fn row_kind(left: &SideRef, right: &SideRef) -> RowKind {
    match (
        matches!(left.kind, SideKind::Git | SideKind::Remote),
        matches!(right.kind, SideKind::Git | SideKind::Remote),
    ) {
        (true, true) => RowKind::GitRefs,
        (false, false) => RowKind::Paths,
        _ => RowKind::Mixed,
    }
}

/// Returns the lower-cased name used to order named rows.
fn name_key(row: &StartRow) -> String {
    row.name.as_deref().unwrap_or_default().to_lowercase()
}
