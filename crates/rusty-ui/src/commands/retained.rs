use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: GLOBAL,
    commands: &[(
        "ferrite_window::OpenRecentlyDeleted",
        "files.recently_deleted",
        "cmd-files-recently-deleted",
    )],
}];
