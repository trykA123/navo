use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: GLOBAL,
    commands: &[
        (
            "ferrite_window::RecordPerformance",
            "perf.record",
            "cmd-perf-record",
        ),
        (
            "ferrite_window::StopRecording",
            "perf.stop",
            "cmd-perf-stop",
        ),
    ],
}];
