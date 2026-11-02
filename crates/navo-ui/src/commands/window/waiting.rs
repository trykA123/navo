use super::Group;

pub(super) const GROUP: Group = Group {
    context: "PaneWaiting",
    commands: &[
        (
            "navo_window::LeaveWaiting",
            "pane.leave_waiting",
            "cmd-pane-leave-waiting",
        ),
        (
            "navo_window::RetryListing",
            "pane.retry_listing",
            "cmd-pane-retry-listing",
        ),
    ],
};
