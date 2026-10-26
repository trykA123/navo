use super::Group;

pub(super) const GROUP: Group = Group {
    context: "CrashNotice",
    commands: &[
        (
            "ferrite_window::OpenRecoveryFolder",
            "crash_notice.open_folder",
            "cmd-crash-notice-open-folder",
        ),
        (
            "ferrite_window::SaveCrashReport",
            "crash_notice.save",
            "cmd-crash-notice-save",
        ),
        (
            "ferrite_window::DismissCrashNotice",
            "crash_notice.dismiss",
            "cmd-crash-notice-dismiss",
        ),
    ],
};
