use super::fmt::{self, close_table, open_table, row};
use super::results::Results;
use super::targets::{Overall, Tally, TargetRow, Verdict};
use super::{Document, html};
use crate::measure::{Bytes, CpuWindow, IdleReport, Metric, StartReport, Stat, Wakeups};
use crate::privacy::Html;

fn tile(page: &mut Html, value: u32, name: &'static str) {
    page.markup("<div class=\"tile\"><b>")
        .uint(u64::from(value))
        .markup("</b>")
        .markup(name)
        .markup("</div>");
}

pub(super) fn summary(page: &mut Html, tally: Tally) {
    html::open_section(page, "summary", "Summary verdict");
    let (class, text) = match tally.overall {
        Overall::AllPass => ("badge pass", "All measured targets pass"),
        Overall::SomeFail => ("badge fail", "Some targets fail"),
        Overall::Incomplete => ("badge", "No target fails, but some were not measured"),
    };
    page.markup("<p><span class=\"")
        .markup(class)
        .markup("\">")
        .markup(text)
        .markup("</span></p><div class=\"tiles\">");
    tile(page, tally.pass, "pass");
    tile(page, tally.fail, "fail");
    tile(page, tally.not_measured, "not measured");
    page.markup("</div>");
    html::close_section(page);
}

fn verdict_cell(page: &mut Html, row: &TargetRow) {
    page.markup("<td>");
    match row.verdict {
        Verdict::Pass => page.markup("<span class=\"badge pass\">Pass</span>"),
        Verdict::Fail => page.markup("<span class=\"badge fail\">Fail</span>"),
        Verdict::NotMeasured { reason } => page
            .markup("<span class=\"badge\">Not measured</span> ")
            .markup("<span class=\"muted\">")
            .text(reason.as_str())
            .markup("</span>"),
    };
    page.markup("</td>");
}

pub(super) fn targets(page: &mut Html, document: &Document) {
    html::open_section(page, "targets", "Targets");
    open_table(
        page,
        "Targets",
        &[
            ("Measure", false),
            ("Result", true),
            ("Target", true),
            ("Verdict", false),
            ("Source", false),
        ],
    );
    for target in &document.targets {
        let result = target
            .value
            .map_or_else(|| "-".into(), |v| fmt::amount(v, target.unit));
        page.markup("<tr><td>")
            .text(target.measure.as_str())
            .markup("</td>");
        page.markup("<td class=\"n\">")
            .text(&result)
            .markup("</td>");
        page.markup("<td class=\"n\">")
            .text(&fmt::limit(target.limit, target.unit, target.strict))
            .markup("</td>");
        verdict_cell(page, target);
        page.markup("<td class=\"muted\">")
            .text(target.source.as_str())
            .markup("</td></tr>");
    }
    close_table(page);
    page.markup("<p class=\"muted\">Medians are compared with each target. Results without a target elsewhere in this report show no target.</p>");
    html::close_section(page);
}

fn stat_row(page: &mut Html, name: &str, metric: &Metric<Stat>) {
    match metric {
        Metric::Measured { value } => {
            let p95 = format!(
                "{}{}",
                fmt::millis(value.p95_micros),
                if value.p95_is_max { " (max)" } else { "" }
            );
            let note = if value.noisy { "noisy" } else { "steady" };
            let runs = value.samples.to_string();
            let median = fmt::millis(value.median_micros);
            row(
                page,
                &[
                    (name, false),
                    (&median, true),
                    (&p95, true),
                    (&runs, true),
                    (note, false),
                ],
            );
        }
        Metric::NotMeasured { reason } => row(
            page,
            &[
                (name, false),
                ("-", true),
                ("-", true),
                ("-", true),
                (reason.as_str(), false),
            ],
        ),
    }
}

fn start_block(
    page: &mut Html,
    title: &'static str,
    note: &'static str,
    start: Option<&StartReport>,
) {
    page.markup("<h3>")
        .markup(title)
        .markup("</h3><p class=\"muted\">")
        .markup(note);
    let Some(start) = start else {
        page.markup(" Not run.</p>");
        return;
    };
    page.markup(" Runs: ")
        .uint(u64::from(start.attempted))
        .markup(", failed: ")
        .uint(u64::from(start.failed))
        .markup(".</p>");
    let heads = [
        ("Step", false),
        ("Median", true),
        ("p95", true),
        ("Runs", true),
        ("Note", false),
    ];
    open_table(page, title, &heads);
    stat_row(page, "First window", &start.first_window);
    stat_row(page, "First frame", &start.first_frame);
    stat_row(page, "First usable", &start.first_usable);
    close_table(page);
}

fn metric_row<T>(page: &mut Html, name: &str, metric: &Metric<T>, show: impl FnOnce(&T) -> String) {
    match metric {
        Metric::Measured { value } => {
            row(page, &[(name, false), (&show(value), true), ("", false)])
        }
        Metric::NotMeasured { reason } => {
            row(
                page,
                &[(name, false), ("-", true), (reason.as_str(), false)],
            );
        }
    }
}

fn bytes_text(bytes: &Bytes) -> String {
    format!(
        "{} private, {} working set",
        fmt::megabytes(bytes.private_bytes),
        fmt::megabytes(bytes.working_set_bytes)
    )
}

fn cpu_text(cpu: &CpuWindow) -> String {
    format!(
        "{} CPU over {}",
        fmt::millis(cpu.cpu_micros),
        fmt::millis(cpu.window_micros)
    )
}

fn wakeups(page: &mut Html, metric: &Metric<Wakeups>) {
    metric_row(page, "Wakeups, total", metric, |w| {
        fmt::rate(w.total_per_second)
    });
    metric_row(page, "Wakeups, busiest thread (VSync)", metric, |w| {
        fmt::rate(w.busiest_thread_per_second)
    });
    metric_row(page, "Wakeups, without busiest thread", metric, |w| {
        fmt::rate(w.without_busiest_per_second)
    });
}

fn idle_block(page: &mut Html, idle: Option<&IdleReport>) {
    page.markup("<h3>Idle</h3>");
    let Some(idle) = idle else {
        page.markup("<p class=\"muted\">Not run.</p>");
        return;
    };
    open_table(
        page,
        "Idle",
        &[("Measure", false), ("Value", true), ("Note", false)],
    );
    metric_row(
        page,
        "Memory after settling",
        &idle.memory_at_settle,
        bytes_text,
    );
    metric_row(page, "CPU, focused", &idle.cpu, cpu_text);
    wakeups(page, &idle.wakeups);
    metric_row(page, "Memory at the end", &idle.memory_at_end, bytes_text);
    close_table(page);
}

pub(super) fn start_and_idle(page: &mut Html, results: &Results) {
    html::open_section(page, "start-idle", "Start and idle");
    start_block(
        page,
        "Cold start",
        "From a fresh copy of the exe; antivirus scans each new file. This is not a reboot-cold start.",
        results.cold.as_ref(),
    );
    start_block(
        page,
        "Warm start",
        "One discarded warm-up run first.",
        results.warm.as_ref(),
    );
    idle_block(page, results.idle.as_ref());
    page.markup("<h3>Exe size</h3>");
    open_table(
        page,
        "Exe size",
        &[("Measure", false), ("Value", true), ("Note", false)],
    );
    match results.exe_size.as_ref() {
        Some(size) => metric_row(page, "Exe size", size, |b| fmt::mebibytes(*b)),
        None => row(
            page,
            &[("Exe size", false), ("-", true), ("not run", false)],
        ),
    }
    close_table(page);
    html::close_section(page);
}
