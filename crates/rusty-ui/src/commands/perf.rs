use super::Group;
use super::contexts::GLOBAL;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: GLOBAL,
    commands: &[
        (
            "settings_sheet::SaveDiagnosticReport",
            "app.save_diagnostic_report",
            "cmd-app-save-diagnostic-report",
        ),
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
