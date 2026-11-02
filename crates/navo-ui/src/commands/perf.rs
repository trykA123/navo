use super::Group;
use super::contexts::GLOBAL;

#[cfg(feature = "tester")]
pub(super) const GROUPS: [Group; 1] = [Group {
    context: GLOBAL,
    commands: &[
        (
            "settings_sheet::SaveDiagnosticReport",
            "app.save_diagnostic_report",
            "cmd-app-save-diagnostic-report",
        ),
        (
            "navo_window::RecordPerformance",
            "perf.record",
            "cmd-perf-record",
        ),
        ("navo_window::StopRecording", "perf.stop", "cmd-perf-stop"),
    ],
}];

#[cfg(not(feature = "tester"))]
pub(super) const GROUPS: [Group; 1] = [Group {
    context: GLOBAL,
    commands: &[],
}];
