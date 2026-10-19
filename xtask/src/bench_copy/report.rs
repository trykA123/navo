use rusty_fs::DriveClass;

use super::fixture::DataSet;

pub(super) struct Measurement {
    pub set: DataSet,
    pub method: &'static str,
    pub seconds: f64,
}

pub(super) fn median(samples: &mut [f64]) -> Option<f64> {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    let upper = *samples.get(middle)?;
    Some(if samples.len().is_multiple_of(2) {
        (samples[middle - 1] + upper) / 2.0
    } else {
        upper
    })
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
         | Set | Method | Median seconds | MB/s |\n\
         | --- | --- | ---: | ---: |\n"
    );
    for row in rows {
        let rate = row.set.bytes() as f64 / 1_000_000.0 / row.seconds;
        output.push_str(&format!(
            "| {} | {} | {:.3} | {rate:.2} |\n",
            row.set.name, row.method, row.seconds
        ));
    }
    output
}
