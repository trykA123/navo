mod options;

use std::{
    fs::{self, OpenOptions},
    ops::ControlFlow,
    path::Path,
    process::{Command, ExitCode},
    time::Instant,
};

use rusty_core::{Location, ProviderPath};
use rusty_fs::{CancelToken, LocalFs, Provider};

use crate::stats::Summary;
use options::{Options, Target};

pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("bench-walk: {error}");
            eprintln!(
                "usage: cargo xtask bench-walk <dir> [--runs N] [--warmup N] [--prepare \"<command>\"] [--targets list,size]"
            );
            return ExitCode::from(2);
        }
    };
    match benchmark(&options, (5_000, 1_000, 99)) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("bench-walk: {error}");
            ExitCode::FAILURE
        }
    }
}

fn benchmark(options: &Options, sizes: (usize, usize, usize)) -> Result<String, String> {
    let mut report = format!(
        "Bench-walk: informational only. Times in milliseconds. Warm-ups: {} (not recorded); runs: {}.\n\n",
        options.warmup, options.runs
    );
    for &target in &options.targets {
        let path = options.root.join(match target {
            Target::List => "A",
            Target::Size => "size-tree",
        });
        let fresh = fixture(&path, target, sizes)?;
        let location = location(&fs::canonicalize(&path).map_err(|error| error.to_string())?)?;
        let mut cold = Vec::new();
        if let Some(command) = &options.prepare {
            for _ in 0..options.runs {
                prepare(command)?;
                cold.push(measure(target, &location)?);
            }
        } else if fresh {
            cold.push(measure(target, &location)?);
        }
        for _ in 0..options.warmup {
            measure(target, &location)?;
        }
        let warm = (0..options.runs)
            .map(|_| measure(target, &location))
            .collect::<Result<Vec<_>, _>>()?;
        report.push_str(&markdown(target, &cold, &warm, options.prepare.is_some()));
    }
    Ok(report)
}

fn measure(target: Target, location: &Location) -> Result<f64, String> {
    let provider = LocalFs::new();
    let cancel = CancelToken::new();
    let started = Instant::now();
    match target {
        Target::List => provider.list(
            location,
            &mut |batch| {
                std::hint::black_box(batch);
                ControlFlow::Continue(())
            },
            &cancel,
        ),
        Target::Size => rusty_fs::folder_size(&provider, location, &cancel).map(|size| {
            std::hint::black_box(size);
        }),
    }
    .map_err(|error| format!("{}: {}", target.name(), error.message()))?;
    Ok(started.elapsed().as_secs_f64() * 1_000.0)
}

fn prepare(command: &str) -> Result<(), String> {
    #[cfg(unix)]
    let status = Command::new("sh").args(["-c", command]).status();
    #[cfg(windows)]
    let status = Command::new("cmd").args(["/C", command]).status();
    let status = status.map_err(|error| format!("starting --prepare: {error}"))?;
    if !status.success() {
        return Err(format!("--prepare failed: {status}"));
    }
    Ok(())
}

fn location(path: &Path) -> Result<Location, String> {
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        ProviderPath::from_bytes(path.as_os_str().as_bytes())
    };
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStrExt;
        ProviderPath::from_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
    };
    Location::local(path).map_err(|error| error.to_string())
}

fn fixture(path: &Path, target: Target, sizes: (usize, usize, usize)) -> Result<bool, String> {
    let fresh = !path.try_exists().map_err(|error| error.to_string())?;
    let result = match target {
        Target::List => files(path, sizes.0),
        Target::Size => {
            directory(path)?;
            for index in 0..sizes.1 {
                files(&path.join(format!("folder-{index:03}")), sizes.2)?;
            }
            check_count(path, sizes.1)
        }
    };
    result.map_err(|error| format!("fixture {}: {error}", path.display()))?;
    Ok(fresh)
}

fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| error.to_string())?;
    if !fs::symlink_metadata(path)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err(format!(
            "{} must be a directory, not a symlink",
            path.display()
        ));
    }
    Ok(())
}

fn files(path: &Path, count: usize) -> Result<(), String> {
    directory(path)?;
    for index in 0..count {
        let file = path.join(format!("file-{index:04}.txt"));
        match fs::symlink_metadata(&file) {
            Ok(metadata) if metadata.is_file() && metadata.len() == 0 => {}
            Ok(_) => return Err(format!("{} must be an empty regular file", file.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&file)
                    .map_err(|error| format!("{}: {error}", file.display()))?;
            }
            Err(error) => return Err(format!("{}: {error}", file.display())),
        }
    }
    check_count(path, count)
}

fn check_count(path: &Path, expected: usize) -> Result<(), String> {
    let count = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .len();
    if count != expected {
        return Err(format!(
            "{} has {count} entries; expected {expected}",
            path.display()
        ));
    }
    Ok(())
}

fn markdown(target: Target, cold: &[f64], warm: &[f64], prepared: bool) -> String {
    let cache = if prepared {
        "prepare command ran before each cold sample"
    } else {
        "cache not flushed; cold only available for a freshly generated fixture"
    };
    let mut report = format!(
        "### {}\n\n{cache}.\n\n| Run | Count | Mean ± σ (ms) | Median (ms) | Min (ms) | Max (ms) | Outliers |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        target.name()
    );
    for (name, samples) in [("cold", cold), ("warm", warm)] {
        if let Some(summary) = Summary::of(samples) {
            report.push_str(&format!(
                "| {name} | {} | {:.3} ± {:.3} | {:.3} | {:.3} | {:.3} | {} |\n",
                summary.count,
                summary.mean,
                summary.sd,
                summary.median,
                summary.min,
                summary.max,
                summary.outliers.len()
            ));
        } else {
            report.push_str(&format!(
                "| {name} (fixture reused) | 0 | N/A | N/A | N/A | N/A | 0 |\n"
            ));
        }
    }
    report.push('\n');
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_renders_fixed_sample_statistics() {
        let report = markdown(Target::List, &[1.0], &[1.0, 2.0, 3.0], false);
        assert!(report.contains(
            "| Run | Count | Mean ± σ (ms) | Median (ms) | Min (ms) | Max (ms) | Outliers |"
        ));
        assert!(report.contains("| warm | 3 | 2.000 ± 1.000 | 2.000 | 1.000 | 3.000 | 0 |"));
        assert!(report.contains("cache not flushed"));
        let report = markdown(Target::Size, &[], &[10.0, 100.0, 11.0, 9.0, 12.0], false);
        assert!(report.contains("cold (fixture reused) | 0 | N/A"));
        assert!(report.contains("| 9.000 | 100.000 | 1 |"));
    }

    #[test]
    fn tiny_fixtures_are_complete_repaired_and_reused_without_false_cold_samples() {
        let path = std::env::temp_dir().join(format!("navo-bench-walk-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let options = Options {
            root: path.clone(),
            runs: 3,
            warmup: 1,
            prepare: None,
            targets: vec![Target::List, Target::Size],
        };
        let report = benchmark(&options, (5, 2, 3)).unwrap();
        assert_eq!(report.matches("| cold | 1 |").count(), 2);
        check_count(&path.join("A"), 5).unwrap();
        check_count(&path.join("size-tree"), 2).unwrap();
        check_count(&path.join("size-tree/folder-000"), 3).unwrap();
        let tree = location(&path.join("size-tree")).unwrap();
        let size = rusty_fs::folder_size(&LocalFs::new(), &tree, &CancelToken::new()).unwrap();
        assert_eq!((size.files, size.folders, size.skipped), (6, 2, 0));
        assert_eq!(
            benchmark(&options, (5, 2, 3))
                .unwrap()
                .matches("cold (fixture reused)")
                .count(),
            2
        );
        fs::remove_file(path.join("A/file-0000.txt")).unwrap();
        assert!(!fixture(&path.join("A"), Target::List, (5, 2, 3)).unwrap());
        fs::write(path.join("A/file-0000.txt"), b"keep").unwrap();
        assert!(fixture(&path.join("A"), Target::List, (5, 2, 3)).is_err());
        assert_eq!(fs::read(path.join("A/file-0000.txt")).unwrap(), b"keep");
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn prepare_runs_before_every_cold_sample_and_failure_is_reported() {
        let path = std::env::temp_dir().join(format!("navo-bench-prepare-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let marker = path.join("prepared");
        let mut options = Options {
            root: path.clone(),
            runs: 3,
            warmup: 1,
            prepare: Some(format!("echo ready >> '{}'", marker.display())),
            targets: vec![Target::List],
        };
        let report = benchmark(&options, (2, 1, 1)).unwrap();
        assert!(report.contains("| cold | 3 |"));
        assert!(report.contains("| warm | 3 |"));
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 3);
        options.prepare = Some("exit 7".to_owned());
        assert!(
            benchmark(&options, (2, 1, 1))
                .unwrap_err()
                .contains("--prepare failed")
        );
        fs::remove_dir_all(path).unwrap();
    }
}
