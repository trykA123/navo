mod fixture;
mod incremental;
mod indexing;
mod listing;
mod report;
mod search;
mod workspace;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use rusty_core::Location;

use fixture::{HUGE, Plan};
use incremental::Changes;
use report::Report;
use workspace::Workspace;

const DEFAULT_FILES: usize = 2_000_000;
const MIN_FILES: usize = 1_000;
const NO_SUBTREE_WATCH: &str = "the folder watcher on this platform covers only the top folder \
    (inotify has no subtree mode), so the walk index does not follow changes in subfolders";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    pub(crate) root: PathBuf,
    pub(crate) files: usize,
    pub(crate) keep: bool,
    pub(crate) out: Option<PathBuf>,
}

pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Options, String> {
    let (mut root, mut files, mut keep, mut out) = (None, DEFAULT_FILES, false, None);
    while let Some(arg) = args.next() {
        let flag = arg.to_string_lossy().into_owned();
        let mut value = || {
            args.next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("bench {flag} needs a value"))
        };
        match flag.as_str() {
            "--root" => root = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--files" => files = count(&value()?)?,
            "--keep" => keep = true,
            _ => return Err(format!("unknown bench argument {flag}")),
        }
    }
    let root = root.ok_or("bench needs --root <folder>")?;
    Ok(Options {
        root,
        files,
        keep,
        out,
    })
}

fn count(text: &OsString) -> Result<usize, String> {
    let digits: String = text
        .to_string_lossy()
        .chars()
        .filter(|ch| !matches!(ch, '_' | ',' | '.'))
        .collect();
    match digits.parse::<usize>() {
        Ok(files) if files >= MIN_FILES => Ok(files),
        _ => Err(format!(
            "bench --files needs a whole number of at least {MIN_FILES}"
        )),
    }
}

pub(crate) fn run(options: &Options) -> ExitCode {
    match execute(options) {
        Ok((report, json, md)) => {
            println!("{}", report::markdown(&report));
            println!("navo bench: wrote {} and {}", json.display(), md.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("navo bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(options: &Options) -> Result<(Report, PathBuf, PathBuf), String> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let (json, md) = report::paths(options.out.as_deref(), &stamp)
        .map_err(|error| format!("finding the report folder: {error}"))?;
    let workspace = Workspace::prepare(&options.root).map_err(|error| error.to_string())?;
    let written = measure(options, &workspace).and_then(|report| {
        report::write(&report, &json, &md).map_err(|error| error.to_string())?;
        Ok(report)
    });
    let removed = workspace.remove(options.keep);
    let report = written?;
    removed.map_err(|error| error.to_string())?;
    Ok((report, json, md))
}

fn location(path: &Path) -> Result<Location, String> {
    crate::startup::path_location(path)
        .ok_or_else(|| format!("{} is not a usable folder path", path.display()))
}

fn listings(workspace: &Workspace) -> Result<Vec<report::Listing>, String> {
    HUGE.iter()
        .map(|name| listing::run(&location(&workspace.fixture.join(name))?, name))
        .collect()
}

fn measure(options: &Options, workspace: &Workspace) -> Result<Report, String> {
    let plan = Plan::new(options.files);
    eprintln!(
        "navo bench: creating {} files in {}",
        plan.files,
        workspace.fixture.display()
    );
    let started = Instant::now();
    fixture::create(&workspace.fixture, &plan).map_err(|error| error.to_string())?;
    let generation = report::Generation {
        files: plan.files,
        folders: plan.folders(),
        ms: report::ms(started.elapsed()),
    };
    let root = location(&workspace.fixture)?;
    eprintln!("navo bench: indexing");
    let (host, index) = indexing::run(&root, &workspace.index)?;
    eprintln!("navo bench: searching");
    let (search_quiet_wait_ms, search) = search::run_when_quiet(&root, &host);
    eprintln!("navo bench: listing the huge folders");
    let listing = listings(workspace)?;
    let changes = Changes::of(plan.operations);
    let incremental = if cfg!(windows) {
        eprintln!("navo bench: changing {} files", plan.operations);
        incremental::run(&workspace.fixture, &host, changes)?
    } else {
        incremental::not_measured(changes, NO_SUBTREE_WATCH)
    };
    if incremental.measured && incremental.reflected_after_ms.is_none() {
        std::mem::forget(host);
    } else {
        drop(host);
    }
    Ok(Report {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: env!("NAVO_COMMIT").to_owned(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        cpus: std::thread::available_parallelism().map_or(0, |count| count.get()),
        root: options.root.display().to_string(),
        files_requested: options.files,
        seed: format!("{:#x}", fixture::SEED),
        fixture_kept: options.keep,
        generation,
        index,
        search_quiet_wait_ms,
        search,
        listing,
        incremental,
    })
}

#[cfg(test)]
mod tests;
