use std::{fs, hint::black_box, path::Path, time::Instant};

use rusty_index::{
    file::{self, Header},
    search::{self, Term},
    table::{Flags, NO_PARENT, NameTable},
};

use super::{Options, Target, markdown, samples};

pub(super) fn benchmark(options: &Options, target: Target, names: usize) -> Result<String, String> {
    let table = synthetic_names(names, target != Target::Search)?;
    if target == Target::Search {
        let (cold, warm) = samples(options, true, || {
            let started = Instant::now();
            for query in ["doc", "document", "0000042", ".rs", "missing"] {
                black_box(search::search(&table, &[Term::Name(query)], 50, &|| false));
            }
            Ok(started.elapsed().as_secs_f64() * 1_000.0)
        })?;
        return Ok(markdown(
            target,
            (&cold, &warm),
            options.prepare.is_some(),
            None,
        ));
    }
    snapshot(options, target, &table)
}

fn synthetic_names(count: usize, rooted: bool) -> Result<NameTable, String> {
    let mut table = NameTable::with_capacity(count + usize::from(rooted), count * 24);
    let parent = if rooted {
        table
            .push(NO_PARENT, "bench-index", Flags::FOLDER)
            .map_err(|error| error.to_string())?
    } else {
        NO_PARENT
    };
    for id in 0..count {
        table
            .push(parent, &format!("document-{id:07}.rs"), Flags::NONE)
            .map_err(|error| error.to_string())?;
    }
    Ok(table)
}

fn snapshot(options: &Options, target: Target, table: &NameTable) -> Result<String, String> {
    fs::create_dir_all(&options.root).map_err(|error| error.to_string())?;
    let directory = options.root.join(format!(
        ".bench-index-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let path = directory.join("index.bin");
    let header = Header {
        volume_serial: 0,
        journal_id: 0,
        next_usn: 0,
        root: "bench-index".to_owned(),
        built_at: 0,
    };
    let result = (|| {
        if target == Target::Load {
            file::save(&path, &header, table).map_err(|error| format!("load fixture: {error}"))?;
        }
        let (cold, warm) = samples(options, true, || {
            measure_snapshot(target, &path, &header, table)
        })?;
        Ok(markdown(
            target,
            (&cold, &warm),
            options.prepare.is_some(),
            None,
        ))
    })();
    fs::remove_dir_all(&directory)
        .map_err(|error| format!("removing {}: {error}", directory.display()))?;
    result
}

fn measure_snapshot(
    target: Target,
    path: &Path,
    header: &Header,
    table: &NameTable,
) -> Result<f64, String> {
    let started = Instant::now();
    match target {
        Target::Save => file::save(path, header, table),
        Target::Load => file::load(path).map(|loaded| {
            black_box(loaded);
        }),
        _ => return Err("expected a snapshot target".to_owned()),
    }
    .map_err(|error| format!("{}: {error}", target.name()))?;
    Ok(started.elapsed().as_secs_f64() * 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_index_targets_return_cold_and_warm_samples() {
        for target in [Target::Walk, Target::Search, Target::Save, Target::Load] {
            let path = std::env::temp_dir().join(format!(
                "navo-bench-index-{}-{}",
                std::process::id(),
                target.name()
            ));
            fs::create_dir(&path).unwrap();
            let options = Options {
                root: path.clone(),
                runs: 2,
                warmup: 1,
                prepare: None,
                targets: if target == Target::Walk {
                    vec![Target::Walk, Target::WalkDua, Target::WalkIgnore]
                } else {
                    vec![target]
                },
            };
            let report = super::super::benchmark(&options, (5, 2, 3), 10).unwrap();
            if target == Target::Walk {
                print!("{report}");
                assert!(report.contains("### walk-dua\n"));
                assert!(report.contains("### walk-ignore\n"));
            }
            assert!(report.contains(&format!("### {}\n", target.name())));
            assert!(report.contains("| cold | 1 |"));
            assert!(report.contains("| warm | 2 |"));
            assert_eq!(
                fs::read_dir(&path).unwrap().count(),
                usize::from(target == Target::Walk)
            );
            fs::remove_dir_all(path).unwrap();
        }
    }
}
