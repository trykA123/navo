//! Sample pane data for the gallery's window page: the tabs, path, cursor and marks of each side.

/// One pane's sample state: its tab names, path, cursor row and marked rows.
pub(super) struct Side {
    pub(super) tabs: &'static [&'static str],
    pub(super) path: &'static str,
    pub(super) cursor: u64,
    pub(super) marked: &'static [u64],
}

/// The left and right panes the window page draws.
pub(super) const SIDES: [Side; 2] = [
    Side {
        tabs: &["commander", "Downloads", "Steam"],
        path: r"C:\work\commander",
        cursor: 9,
        marked: &[7, 11],
    },
    Side {
        tabs: &["2026-10", "photos"],
        path: r"Z:\archive\2026-10",
        cursor: 4,
        marked: &[],
    },
];
