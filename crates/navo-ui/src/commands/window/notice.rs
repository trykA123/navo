use super::Group;

pub(super) const GROUP: Group = Group {
    context: "CrashNotice",
    commands: &[
        (
            "navo_window::OpenRecoveryFolder",
            "crash_notice.open_folder",
            "cmd-crash-notice-open-folder",
        ),
        (
            "navo_window::SaveCrashReport",
            "crash_notice.save",
            "cmd-crash-notice-save",
        ),
        (
            "navo_window::DismissCrashNotice",
            "crash_notice.dismiss",
            "cmd-crash-notice-dismiss",
        ),
    ],
};
