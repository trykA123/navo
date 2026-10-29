use std::{
    fs,
    hint::black_box,
    path::Path,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use rusty_fs::{CancelToken, LocalFs};
use rusty_index::walker::{self, Limits, Outcome};

use super::{Options, Target, fixture, location, markdown, samples};

pub(super) fn benchmark(
    options: &Options,
    target: Target,
    sizes: (usize, usize, usize),
) -> Result<String, String> {
    let path = options.root.join("size-tree");
    let fresh = fixture(&path, target, sizes)?;
    let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
    let files = sizes.1 * sizes.2;
    let expected = 1 + sizes.1 + files;
    let (cold, warm) = samples(options, fresh, || {
        let started = Instant::now();
        let count = match target {
            Target::Walk => measure_walk(&path),
            Target::WalkDua => measure_dua(&path),
            Target::WalkIgnore => measure_ignore(&path),
            _ => return Err("expected a walker target".to_owned()),
        }?;
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        check_count(target, count, expected)?;
        Ok(elapsed)
    })?;
    if target != Target::Walk {
        check_count(target, expected, measure_walk(&path)?)?;
    }
    Ok(markdown(
        target,
        (&cold, &warm),
        options.prepare.is_some(),
        Some(files),
    ))
}

fn check_count(target: Target, count: usize, expected: usize) -> Result<(), String> {
    if count != expected {
        return Err(format!(
            "{} counted {count} entries; walk expected {expected}",
            target.name()
        ));
    }
    Ok(())
}

fn measure_walk(path: &Path) -> Result<usize, String> {
    let root = location(path)?;
    let result = walker::walk(
        &LocalFs::new(),
        &root,
        Limits {
            max_entries: usize::MAX,
            exclude: Vec::new(),
        },
        &CancelToken::new(),
        |_, _| {},
    )
    .map_err(|error| format!("walk: {error}"))?;
    if result.outcome != Outcome::Complete
        || result.progress.skipped != 0
        || result.progress.depth_limited != 0
    {
        return Err(format!("walk incomplete: {:?}", result.progress));
    }
    let count = result.progress.entries;
    black_box(result);
    Ok(count)
}

fn measure_dua(path: &Path) -> Result<usize, String> {
    let mut count = 0;
    for entry in dua_core::walk(
        path,
        4,
        dua_core::Order::Completion,
        dua_core::Options::default().skip_metadata(),
        |_| true,
    ) {
        black_box(entry.map_err(|error| format!("walk-dua: {error}"))?);
        count += 1;
    }
    Ok(count)
}

fn measure_ignore(path: &Path) -> Result<usize, String> {
    let count = AtomicUsize::new(0);
    let error = Mutex::new(None);
    ignore::WalkBuilder::new(path)
        .standard_filters(false)
        .threads(4)
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                match entry {
                    Ok(entry) => {
                        black_box(entry);
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(cause) => {
                        *error.lock().unwrap_or_else(PoisonError::into_inner) =
                            Some(cause.to_string());
                    }
                }
                ignore::WalkState::Continue
            })
        });
    if let Some(error) = error.into_inner().unwrap_or_else(PoisonError::into_inner) {
        return Err(format!("walk-ignore: {error}"));
    }
    Ok(count.into_inner())
}
