use super::fmt::{self, close_table, count, open_table, row};
use super::results::Results;
use super::timings::Timings;
use super::{html, labels};
use crate::index::IndexReport;
use crate::machine::MachineSummary;
use crate::privacy::{Html, Report};

fn optional_millis(micros: Option<u64>) -> String {
    micros.map_or_else(|| "-".into(), fmt::millis)
}

fn index_numbers(page: &mut Html, index: &IndexReport) {
    open_table(page, "Index", &[("Measure", false), ("Value", true)]);
    let state = labels::service(index.service);
    let answer = |micros| optional_millis(micros);
    let rows = [
        ("Service", state.to_string()),
        ("Service start, cold", answer(index.cold_start_micros)),
        ("Service start, warm", answer(index.warm_start_micros)),
        ("First answer", answer(index.first_answer_micros)),
        (
            "First answer while building",
            answer(index.first_building_answer_micros),
        ),
        ("Entries", count(index.entries)),
        (
            "Building",
            if index.building { "yes" } else { "no" }.to_string(),
        ),
    ];
    for (name, value) in &rows {
        row(page, &[(name, false), (value, true)]);
    }
    close_table(page);
}

fn index_queries(page: &mut Html, index: &IndexReport) {
    if index.queries.is_empty() {
        return;
    }
    page.markup("<h3>Query latency</h3><p class=\"muted\">Service, without UI. Queries are named by position, never by text.</p>");
    let heads = [
        ("Query", false),
        ("Runs", true),
        ("Min", true),
        ("Median", true),
        ("Max", true),
        ("Results", true),
    ];
    open_table(page, "Query latency", &heads);
    for query in &index.queries {
        let name = format!("q{}", u32::from(query.id) + 1);
        let cells = [
            fmt::millis(query.min_micros),
            fmt::millis(query.median_micros),
            fmt::millis(query.max_micros),
        ];
        let runs = query.runs.to_string();
        let results = count(query.results);
        row(
            page,
            &[
                (&name, false),
                (&runs, true),
                (&cells[0], true),
                (&cells[1], true),
                (&cells[2], true),
                (&results, true),
            ],
        );
    }
    close_table(page);
}

fn index_memory(page: &mut Html, index: &IndexReport) {
    if index.memory.parts.is_empty() {
        return;
    }
    page.markup("<h3>Service memory</h3>");
    open_table(
        page,
        "Service memory",
        &[("Part", false), ("Size", true), ("Items", true)],
    );
    for part in &index.memory.parts {
        let items = count(part.items);
        row(
            page,
            &[
                (labels::memory_part(part.part), false),
                (&fmt::megabytes(part.bytes), true),
                (&items, true),
            ],
        );
    }
    close_table(page);
}

pub(super) fn index(page: &mut Html, results: &Results) {
    html::open_section(page, "index", "Index");
    match results.index.as_ref() {
        Some(index) => {
            index_numbers(page, index);
            index_queries(page, index);
            index_memory(page, index);
        }
        None => {
            page.markup("<p class=\"muted\">The index check did not run.</p>");
        }
    }
    html::close_section(page);
}

pub(super) fn timings(page: &mut Html, results: &Results) {
    html::open_section(page, "timings", "Folder timings");
    match &results.timings {
        Timings::NotMeasured { reason } => {
            page.markup("<p class=\"muted\">Not measured: ")
                .text(reason.as_str())
                .markup(".</p>");
        }
        Timings::Measured { rows } => {
            let heads = [
                ("Folder", false),
                ("Step", false),
                ("No.", true),
                ("Median", true),
                ("p95", true),
                ("Runs", true),
            ];
            open_table(page, "Folder timings", &heads);
            for timing in rows {
                let cells = [
                    timing.index.to_string(),
                    fmt::millis(timing.median_micros),
                    fmt::millis(timing.p95_micros),
                    timing.samples.to_string(),
                ];
                row(
                    page,
                    &[
                        (labels::folder(timing.folder), false),
                        (labels::step(timing.step), false),
                        (&cells[0], true),
                        (&cells[1], true),
                        (&cells[2], true),
                        (&cells[3], true),
                    ],
                );
            }
            close_table(page);
        }
    }
    html::close_section(page);
}

fn histogram(page: &mut Html, group: &str, buckets: &[Bucket]) {
    let widest = buckets.iter().map(|b| b.1).max().unwrap_or(0).max(1);
    page.markup("<h3>")
        .text(group)
        .markup("</h3><div class=\"hist\">");
    for (bucket, files, bytes) in buckets {
        page.markup("<span>").text(bucket).markup("</span>");
        page.markup("<span class=\"track\"><i style=\"width:")
            .uint(files * 100 / widest)
            .markup("%\"></i></span>");
        let size = fmt::megabytes(*bytes);
        page.markup("<span class=\"n\" title=\"")
            .text(&size)
            .markup("\">")
            .text(&count(*files))
            .markup("</span>");
    }
    page.markup("</div>");
}

type Bucket<'a> = (&'a str, u64, u64);

fn histograms(page: &mut Html, results: &Results) {
    let mut groups: Vec<(&str, Vec<Bucket>)> = Vec::new();
    for report in &results.profile {
        let Report::Bucket {
            group,
            bucket,
            files,
            bytes,
        } = report
        else {
            continue;
        };
        let entry = (bucket.as_str(), *files, *bytes);
        match groups.iter_mut().find(|(name, _)| *name == group.as_str()) {
            Some((_, buckets)) => buckets.push(entry),
            None => groups.push((group.as_str(), vec![entry])),
        }
    }
    for (group, buckets) in &groups {
        histogram(page, group, buckets);
    }
}

fn counts_and_extensions(page: &mut Html, results: &Results) {
    open_table(page, "Counts", &[("Measure", false), ("Value", true)]);
    for report in &results.profile {
        if let Report::Measurement { label, value } = report {
            row(page, &[(label.as_str(), false), (&count(*value), true)]);
        }
    }
    close_table(page);
    page.markup("<h3>Extensions</h3><p class=\"muted\">An extension shows only when seen on at least 10 files in 2 folders; the rest is other.</p>");
    open_table(
        page,
        "Extensions",
        &[("Extension", false), ("Files", true), ("Size", true)],
    );
    for report in &results.profile {
        if let Report::Extension {
            extension,
            files,
            bytes,
        } = report
        {
            row(
                page,
                &[
                    (extension.as_str(), false),
                    (&count(*files), true),
                    (&fmt::megabytes(*bytes), true),
                ],
            );
        }
    }
    close_table(page);
}

pub(super) fn profile(page: &mut Html, results: &Results) {
    html::open_section(page, "profile", "Data profile");
    if results.profile.is_empty() {
        page.markup("<p class=\"muted\">No folder was profiled.</p>");
    } else {
        counts_and_extensions(page, results);
        histograms(page, results);
    }
    html::close_section(page);
}

fn machine_rows(machine: &MachineSummary) -> [(&'static str, String); 5] {
    let number = |value: Option<u32>| value.map_or_else(|| "unknown".into(), |v| v.to_string());
    [
        ("CPU cores", machine.cpu_cores.to_string()),
        ("RAM", ram_text(machine.ram_gib)),
        ("Disk", labels::disk(machine.disk).to_string()),
        (
            "Free space",
            labels::free_space(machine.free_space).to_string(),
        ),
        ("Windows build", number(machine.windows_build)),
    ]
}

pub(super) fn machine(page: &mut Html, results: &Results) {
    html::open_section(page, "machine", "Machine");
    match results.machine.as_ref() {
        Some(machine) => {
            open_table(page, "Machine", &[("Fact", false), ("Value", true)]);
            for (name, value) in &machine_rows(machine) {
                row(page, &[(name, false), (value, true)]);
            }
            row(
                page,
                &[
                    ("Defender", false),
                    (labels::defender(machine.defender), true),
                ],
            );
            close_table(page);
        }
        None => {
            page.markup("<p class=\"muted\">Machine facts were not read.</p>");
        }
    }
    html::close_section(page);
}

pub(super) fn baselines(page: &mut Html) {
    html::open_section(page, "baselines", "Baselines");
    page.markup("<p class=\"muted\">Library baselines are not measured in v1: later.</p>");
    html::close_section(page);
}

fn ram_text(gib: Option<u32>) -> String {
    gib.map_or_else(|| "unknown".into(), |g| format!("{g} GiB"))
}
