use rusty_fs::DriveClass;

use crate::stats::Summary;

use super::fixture::DataSet;

pub(super) struct Measurement {
    pub set: DataSet,
    pub method: &'static str,
    pub seconds: Vec<f64>,
}

pub(super) fn markdown(
    rows: &[Measurement],
    classes: [DriveClass; 2],
    workers: usize,
    runs: usize,
) -> String {
    let [source, target] = classes;
    let mut output = format!(
        "Drive classes: source {source:?}; target {target:?}. Auto workers: {workers}.\n\n\
         {runs} run(s) per method; median elapsed time. MB = 1,000,000 bytes.\n\n\
         The destination is removed between runs; OS caches are not flushed.\n\n\
         | Set | Method | Median seconds | MB/s | Mean ± σ seconds | Outliers |\n\
         | --- | --- | ---: | ---: | ---: | ---: |\n"
    );
    for row in rows {
        let Some(summary) = Summary::of(&row.seconds) else {
            continue;
        };
        let rate = row.set.bytes() as f64 / 1_000_000.0 / summary.median;
        output.push_str(&format!(
            "| {} | {} | {:.3} | {rate:.2} | {:.3} ± {:.3} | {} |\n",
            row.set.name,
            row.method,
            summary.median,
            summary.mean,
            summary.sd,
            summary.outliers.len()
        ));
    }
    output
}
