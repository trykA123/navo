use std::fmt::Write;

use super::ReportInput;

pub(super) const SECTIONS: [&str; 7] = [
    "build",
    "system",
    "configuration",
    "errors",
    "recent_events",
    "memory",
    "performance",
];

const PRIVACY: &str = "This report holds no file names, user or computer names; paths are replaced by tokens like <d:3fa2>.";

pub(super) fn summary_text(input: &ReportInput) -> String {
    let memory = input.memory.as_ref().map_or_else(
        || "not available".to_owned(),
        |m| format!("{} caches, {} task owners", m.caches.len(), m.tasks.len()),
    );
    let mut text = String::from("Ferrite diagnostic report (report.json)\n\n");
    let lines = [
        "Build: version and commit.".to_owned(),
        "System: OS, CPU count, RAM, GPU, display scale and DPI awareness.".to_owned(),
        format!(
            "Configuration: {} changed settings.",
            input.settings_changed.len()
        ),
        format!("Errors: {}.", input.errors.len()),
        format!("Recent events: {}.", input.events.len()),
        format!("Memory: {memory}."),
        format!("Performance: {} start-up marks.", input.startup_ms.len()),
        format!("Redaction misses: {}.", input.redaction_misses),
    ];
    for line in lines {
        let _ = writeln!(text, "{line}");
    }
    let _ = writeln!(text, "\n{PRIVACY}");
    text
}
