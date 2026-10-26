use serde::Deserialize;

const MB: f64 = 1_000_000.0;
const WARM_TARGET_MS: f64 = 150.0;
const PRIVATE_TARGET_BYTES: u64 = 80_000_000;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct Report {
    version: String,
    commit: String,
    exe_bytes: Option<u64>,
    renderer: String,
    pub(super) first_frame_ms: f64,
    pub(super) first_usable_ms: f64,
    pub(super) idle_private_bytes: Option<u64>,
    pub(super) idle_working_set_bytes: Option<u64>,
    pub(super) peak_working_set_bytes: Option<u64>,
}

pub(super) fn parse(json: &str) -> Result<Report, String> {
    let report: Report = serde_json::from_str(json).map_err(|error| error.to_string())?;
    for value in [report.first_frame_ms, report.first_usable_ms] {
        if !value.is_finite() || value < 0.0 {
            return Err("frame times must be finite and nonnegative".to_owned());
        }
    }
    Ok(report)
}

fn stats(values: impl Iterator<Item = f64>) -> [f64; 3] {
    let mut values: Vec<_> = values.collect();
    values.sort_by(f64::total_cmp);
    [
        values[0],
        values[values.len() / 2],
        values[values.len() - 1],
    ]
}

pub(super) fn guard(first: &Report, warm: &[Report]) -> Result<(), String> {
    let median = stats(warm.iter().map(|report| report.first_frame_ms))[1];
    let private = std::iter::once(first)
        .chain(warm)
        .filter_map(|report| report.idle_private_bytes)
        .max()
        .unwrap_or_default();
    let mut failures = Vec::new();
    if median > 2.0 * WARM_TARGET_MS {
        failures.push(format!("median warm start {median:.1} ms > 300 ms"));
    }
    if private > 2 * PRIVATE_TARGET_BYTES {
        failures.push(format!("private bytes {private} > 160000000 bytes"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

pub(super) fn markdown(first: &Report, warm: &[Report], guard: &Result<(), String>) -> String {
    let exe = first.exe_bytes.map_or_else(
        || "N/A".to_owned(),
        |bytes| format!("{bytes} bytes ({:.2} MB)", bytes as f64 / MB),
    );
    let mut output = format!(
        "\n## Navo Windows performance\n\nRenderer: {}\n\nVersion: {} · Commit: {}\n\nExe: {exe}\n\n\
         Microsoft Basic Render Driver uses software rendering; frame times are not absolute.\n\n\
         First is the closest this runner gets to cold; warm is five subsequent launches.\n\n\
         MB = 1,000,000 bytes. Memory is sampled after 5 s idle with two panes. N/A means unavailable.\n\n\
         | Measurement | Run | Min | Median | Max | Target |\n\
         | --- | --- | ---: | ---: | ---: | --- |\n",
        escape(&first.renderer),
        escape(&first.version),
        escape(&first.commit),
    );
    rows(
        &mut output,
        "first",
        std::slice::from_ref(first),
        "≤ 400 ms",
    );
    rows(&mut output, "warm", warm, "≤ 150 ms");
    let verdict = match guard {
        Ok(()) => "passed".to_owned(),
        Err(error) => format!("FAILED: {error}"),
    };
    output.push_str(&format!(
        "\nGuard: {verdict}. Fails only when median warm first frame > 300 ms \
         or any run's private bytes > 160 MB. Other measurements are informational.\n"
    ));
    output
}

fn rows(output: &mut String, run: &str, reports: &[Report], target: &str) {
    type Metric = (&'static str, fn(&Report) -> Option<f64>, &'static str);
    let metrics: [Metric; 5] = [
        ("Start to first frame", |r| Some(r.first_frame_ms), "ms"),
        ("First usable frame", |r| Some(r.first_usable_ms), "ms"),
        ("Private bytes", |r| r.idle_private_bytes.map(to_mb), "MB"),
        ("Working set", |r| r.idle_working_set_bytes.map(to_mb), "MB"),
        (
            "Peak working set",
            |r| r.peak_working_set_bytes.map(to_mb),
            "MB",
        ),
    ];
    for (name, value, unit) in metrics {
        let target = if unit == "ms" { target } else { "< 80 MB" };
        let Some(values) = reports.iter().map(value).collect::<Option<Vec<_>>>() else {
            output.push_str(&format!(
                "| {name} | {run} | N/A | N/A | N/A | {target} |\n"
            ));
            continue;
        };
        let [min, median, max] = stats(values.into_iter());
        output.push_str(&format!(
            "| {name} | {run} | {min:.2} {unit} | {median:.2} {unit} | {max:.2} {unit} | {target} |\n"
        ));
    }
}

fn to_mb(bytes: u64) -> f64 {
    bytes as f64 / MB
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace(['\r', '\n'], " ")
}
