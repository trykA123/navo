use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use super::listing::{self, FILTER};
use super::report::Listing;

const RUNS: usize = 5;
const SYNTHETIC_CHARS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    pub(super) folder: PathBuf,
    pub(super) filter: Option<String>,
    pub(super) out: PathBuf,
}

#[derive(Debug, Serialize)]
struct Run {
    list_ms: f64,
    sort_ms: Vec<(String, f64)>,
    filter_keystroke_ms: Vec<f64>,
}

#[derive(Debug, Serialize)]
struct Summary {
    entries: usize,
    bytes: u64,
    filter_chars: usize,
    filter_matches: usize,
    runs: Vec<Run>,
}

pub(super) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Options, String> {
    let (mut folder, mut filter, mut out) = (None, None, None);
    while let Some(arg) = args.next() {
        let flag = arg.to_string_lossy().into_owned();
        let mut value = || {
            args.next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("bench {flag} needs a value"))
        };
        match flag.as_str() {
            "--folder" => folder = Some(PathBuf::from(value()?)),
            "--filter" => filter = Some(value()?.to_string_lossy().into_owned()),
            "--out" => out = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown bench argument {flag}")),
        }
    }
    Ok(Options {
        folder: folder.ok_or("bench needs --folder <folder>")?,
        filter,
        out: out.ok_or("bench --folder needs --out <file.json>")?,
    })
}

pub(super) fn run(options: &Options) -> ExitCode {
    match measure(options).and_then(|summary| write(&summary, &options.out)) {
        Ok(()) => {
            println!("navo bench: wrote {}", options.out.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("navo bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn top_level_bytes(folder: &Path) -> Result<u64, String> {
    let entries = fs::read_dir(folder).map_err(|error| format!("reading the folder: {error}"))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(fs::Metadata::is_file)
        .map(|meta| meta.len())
        .sum())
}

fn check_out(options: &Options) -> Result<(), String> {
    const REFUSED: &str = "bench --folder: --out must be outside the folder";
    let folder = fs::canonicalize(&options.folder)
        .map_err(|error| format!("reading the folder: {error}"))?;
    if options.out.is_dir() {
        return Err(REFUSED.to_owned());
    }
    let parent = match options.out.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent =
        fs::canonicalize(parent).map_err(|error| format!("reading the --out folder: {error}"))?;
    if parent.starts_with(&folder) {
        return Err(REFUSED.to_owned());
    }
    Ok(())
}

fn measure(options: &Options) -> Result<Summary, String> {
    check_out(options)?;
    let location = super::location(&options.folder)?;
    let filter = options
        .filter
        .clone()
        .unwrap_or_else(|| FILTER.chars().take(SYNTHETIC_CHARS).collect());
    let bytes = top_level_bytes(&options.folder)?;
    let listings = (0..RUNS)
        .map(|_| listing::run(&location, "folder", &filter))
        .collect::<Result<Vec<Listing>, String>>()?;
    let first = listings.first().ok_or("no runs")?;
    Ok(Summary {
        entries: first.entries,
        bytes,
        filter_chars: filter.chars().count(),
        filter_matches: first.filter_matches,
        runs: listings.into_iter().map(run_of).collect(),
    })
}

fn run_of(listing: Listing) -> Run {
    Run {
        list_ms: listing.list_ms,
        sort_ms: listing.sort_ms,
        filter_keystroke_ms: listing.filter_keystroke_ms,
    }
}

fn write(summary: &Summary, out: &Path) -> Result<(), String> {
    let text = serde_json::to_string_pretty(summary).map_err(|error| error.to_string())?;
    fs::write(out, text).map_err(|error| format!("writing the report: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Result<Options, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn folder_arguments_are_read_and_checked() {
        let options = parsed(&["--folder", "f", "--out", "o.json"]).unwrap();
        assert_eq!(options.filter, None);
        let options = parsed(&["--filter", "ab", "--folder", "f", "--out", "o"]).unwrap();
        assert_eq!(options.filter.as_deref(), Some("ab"));
        assert!(parsed(&["--folder", "f"]).unwrap_err().contains("--out"));
        assert!(parsed(&["--folder", "f", "--out"]).is_err());
        assert!(parsed(&["--folder", "f", "--out", "o", "--x"]).is_err());
    }

    #[test]
    fn a_folder_run_writes_numbers_only_and_leaves_the_folder_alone() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("secretfolder");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("sensor-secretname.txt"), b"12345").unwrap();
        fs::write(folder.join("other.bin"), b"123").unwrap();
        let out = dir.path().join("out.json");
        let options = Options {
            folder: folder.clone(),
            filter: Some("secretfilter".to_owned()),
            out: out.clone(),
        };
        let summary = measure(&options).unwrap();
        write(&summary, &out).unwrap();
        let text = fs::read_to_string(&out).unwrap();
        for word in ["secret", "sensor", ".txt", "other"] {
            assert!(!text.contains(word), "{word}");
        }
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["entries"], 2);
        assert_eq!(json["bytes"], 8);
        assert_eq!(json["filter_chars"], 12);
        assert_eq!(json["runs"].as_array().unwrap().len(), RUNS);
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
    }

    fn refused_and_unchanged(folder: &Path, out: PathBuf) {
        let before = names(folder);
        let options = Options {
            folder: folder.to_path_buf(),
            filter: None,
            out,
        };
        let error = measure(&options).unwrap_err();
        assert!(
            error.contains("--out must be outside the folder"),
            "{error}"
        );
        assert_eq!(names(folder), before);
    }

    fn names(folder: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = walk(folder);
        found.sort();
        found
    }

    fn walk(folder: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in fs::read_dir(folder).unwrap().flatten() {
            found.push(entry.path());
            if entry.path().is_dir() {
                found.extend(walk(&entry.path()));
            }
        }
        found
    }

    #[test]
    fn an_out_path_inside_the_folder_is_refused_before_anything_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("f");
        fs::create_dir_all(folder.join("sub")).unwrap();
        fs::write(folder.join("a.txt"), b"1").unwrap();
        refused_and_unchanged(&folder, folder.join("out.json"));
        refused_and_unchanged(&folder, folder.join("sub").join("out.json"));
        refused_and_unchanged(&folder, folder.join("sub"));
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        refused_and_unchanged(&folder, elsewhere);
        #[cfg(unix)]
        {
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(folder.join("sub"), &link).unwrap();
            refused_and_unchanged(&folder, link.join("out.json"));
        }
    }

    #[test]
    fn an_out_path_elsewhere_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("f");
        fs::create_dir(&folder).unwrap();
        let options = Options {
            folder,
            filter: None,
            out: dir.path().join("out.json"),
        };
        assert!(check_out(&options).is_ok());
    }
}
