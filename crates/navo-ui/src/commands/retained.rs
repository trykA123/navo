use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 2] = [
    Group {
        context: GLOBAL,
        commands: &[(
            "navo_window::OpenRecentlyDeleted",
            "files.recently_deleted",
            "cmd-files-recently-deleted",
        )],
    },
    Group {
        context: "RecentlyDeleted",
        commands: &[
            (
                "navo_window::RestoreDeleted",
                "deleted.restore",
                "cmd-deleted-restore",
            ),
            (
                "navo_window::PurgeDeleted",
                "deleted.purge",
                "cmd-deleted-purge",
            ),
        ],
    },
];
