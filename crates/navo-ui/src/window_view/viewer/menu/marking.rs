//! The viewer's mark-colour and bookmark menu entries.

use super::super::{
    ClearMarks, Mark1, Mark2, Mark3, Mark4, Mark5, Mark6, Mark7, Mark8, ToggleBookmark,
};
use super::entries::MenuEntry;

pub(in crate::window_view::viewer) fn marking_entries() -> Vec<MenuEntry> {
    let colours: Vec<MenuEntry> = vec![
        MenuEntry::Run("Mark 1".into(), Box::new(Mark1), false),
        MenuEntry::Run("Mark 2".into(), Box::new(Mark2), false),
        MenuEntry::Run("Mark 3".into(), Box::new(Mark3), false),
        MenuEntry::Run("Mark 4".into(), Box::new(Mark4), false),
        MenuEntry::Run("Mark 5".into(), Box::new(Mark5), false),
        MenuEntry::Run("Mark 6".into(), Box::new(Mark6), false),
        MenuEntry::Run("Mark 7".into(), Box::new(Mark7), false),
        MenuEntry::Run("Mark 8".into(), Box::new(Mark8), false),
        MenuEntry::Break,
        MenuEntry::Run("Clear marks".into(), Box::new(ClearMarks), false),
    ];
    vec![
        MenuEntry::Submenu("Mark colour", colours),
        MenuEntry::Run("Bookmark".into(), Box::new(ToggleBookmark), false),
    ]
}
