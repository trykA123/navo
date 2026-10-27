use super::Group;

pub(super) const GROUP: Group = Group {
    context: "PaneWaiting",
    commands: &[
        (
            "ferrite_window::LeaveWaiting",
            "pane.leave_waiting",
            "cmd-pane-leave-waiting",
        ),
        (
            "ferrite_window::RetryListing",
            "pane.retry_listing",
            "cmd-pane-retry-listing",
        ),
    ],
};
