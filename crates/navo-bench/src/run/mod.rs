//! One whole run: arguments, folder, run root and output folder, the phases in order, the
//! manifest proof, the report, and the run root to the Recycle Bin.
//!
//! The machine sits behind `Platform`, so the whole sequence runs against fakes.

mod output;
mod phases;
#[cfg(windows)]
mod real;
#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::guard::{OutputFolder, Recycler, ScratchRoot};
use crate::harness::args::{self, Args};
use crate::harness::picker::PickerError;
use crate::harness::probe::{ProbeError, probe_in};
use crate::harness::start::{StartError, choose_folder, open_folder};
use crate::index::IndexReport;
use crate::machine::MachineSummary;
use crate::privacy::{LeakScanner, SafeText};
use crate::source::ReadOnlySource;
use crate::{measure, safe_text, timings};

use output::Console;
#[cfg(windows)]
pub use real::RealPlatform;

/// Everything the run needs from the machine; tests inject fakes.
pub trait Platform {
    /// The folder that holds navo-bench; navo.exe is looked for here.
    fn exe_dir(&self) -> Option<PathBuf>;
    fn pick_folder(&mut self) -> Result<Option<PathBuf>, PickerError>;
    fn recycler(&self) -> &dyn Recycler;
    fn measure_host<'a>(&self, root: &'a ScratchRoot, exe: &Path) -> Box<dyn measure::Host + 'a>;
    fn timings_host<'a>(
        &self,
        root: &'a ScratchRoot,
        exe: &Path,
        owner: Option<ReadOnlySource>,
    ) -> Box<dyn timings::Host + 'a>;
    /// The index check, or why it could not run.
    fn index(&mut self, root: &ScratchRoot, exe: &Path) -> Result<IndexReport, SafeText>;
    fn machine(&self, root: &Path) -> MachineSummary;
}

/// How the run ended; `code` is the process exit code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exit {
    /// Every phase ran; some may be "not measured" with a reason.
    Done,
    /// A phase failed, or the report or cleanup failed.
    Failed,
    /// Bad arguments, a refused folder, or nothing could be set up.
    Usage,
    /// The owner's folder changed or could not be proved unchanged.
    DataSafety,
}

impl Exit {
    pub fn code(self) -> u8 {
        match self {
            Self::Done => 0,
            Self::Failed => 1,
            Self::Usage => 2,
            Self::DataSafety => 3,
        }
    }
}

/// What the phases work on once setup has passed.
struct Setup {
    root: ScratchRoot,
    out: OutputFolder,
    source: Option<ReadOnlySource>,
    exe: Result<PathBuf, ProbeError>,
    quick: bool,
}

/// Runs navo-bench with the arguments after the program name; progress goes to `console`.
pub fn run(args: Vec<OsString>, platform: &mut impl Platform, console: &mut impl Write) -> Exit {
    let Ok(identity) = LeakScanner::new(&[]) else {
        return Exit::Usage;
    };
    let mut say = Console::new(&mut *console, identity);
    let Ok(args) = args::parse(args) else {
        say.line(output::USAGE);
        return Exit::Usage;
    };
    let folder = match choose_folder(&args, || platform.pick_folder()) {
        Ok(folder) => folder.map(std::path::absolute).transpose(),
        Err(error) => {
            say.start_error(error);
            return Exit::Usage;
        }
    };
    let Ok(folder) = folder else {
        say.line(output::FOLDER_UNREADABLE);
        return Exit::Usage;
    };
    if folder.is_none() {
        say.start_error(StartError::Cancelled);
    }
    let planted: Vec<String> = folder
        .iter()
        .map(|folder| folder.to_string_lossy().into_owned())
        .collect();
    let planted: Vec<&str> = planted.iter().map(String::as_str).collect();
    let Ok(scanner) = LeakScanner::new(&planted) else {
        say.line(safe_text!("the folder path is too long to protect"));
        return Exit::Usage;
    };
    drop(say);
    let mut say = Console::new(&mut *console, scanner.clone());
    let Some(setup) = set_up(&args, folder.as_deref(), platform, &mut say) else {
        return Exit::Usage;
    };
    drop(say);
    finish(setup, platform, console, &scanner)
}

fn set_up(
    args: &Args,
    folder: Option<&Path>,
    platform: &mut impl Platform,
    say: &mut Console<impl Write>,
) -> Option<Setup> {
    let Some(exe_dir) = platform.exe_dir() else {
        say.line(safe_text!("navo-bench cannot find its own folder"));
        return None;
    };
    let out = match &args.out {
        Some(out) => std::path::absolute(out).ok(),
        None => Some(exe_dir.join(output::default_name(SystemTime::now()))),
    };
    let Some((out, parent)) = out.and_then(|out| Some((out.clone(), out.parent()?.to_owned())))
    else {
        say.line(output::OUT_REFUSED);
        return None;
    };
    let root = match ScratchRoot::create(&parent, folder) {
        Ok(root) => root,
        Err(error) => {
            say.guard_error(output::OUT_REFUSED, error);
            return None;
        }
    };
    if let Ok(stale) = root.clean_stale_runs(&parent, platform.recycler()) {
        say.stale(stale);
    }
    let source = match folder
        .map(|folder| open_folder(folder, root.path()))
        .transpose()
    {
        Ok(source) => source,
        Err(error) => return abandon(root, platform, say, |say| say.start_error(error)),
    };
    let out = match OutputFolder::create(&out, folder) {
        Ok(out) => out,
        Err(error) => {
            return abandon(root, platform, say, |say| {
                say.guard_error(output::OUT_REFUSED, error);
            });
        }
    };
    Some(Setup {
        root,
        out,
        source,
        exe: probe_in(&exe_dir),
        quick: args.quick,
    })
}

fn abandon<W: Write>(
    root: ScratchRoot,
    platform: &impl Platform,
    say: &mut Console<W>,
    explain: impl FnOnce(&mut Console<W>),
) -> Option<Setup> {
    explain(say);
    if let Err(error) = root.remove_root(platform.recycler()) {
        say.guard_error(output::CLEANUP_FAILED, error);
    }
    None
}

/// Runs the phases, then publishes the report in its own guarded step; the run root is
/// recycled even after a panic.
fn finish(
    setup: Setup,
    platform: &mut impl Platform,
    console: &mut impl Write,
    scanner: &LeakScanner,
) -> Exit {
    let Setup {
        root,
        out,
        source,
        exe,
        quick,
    } = setup;
    let inputs = phases::Inputs {
        root: &root,
        source: source.as_ref(),
        exe: exe.as_deref().map_err(|error| *error),
        quick,
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        phases::run_all(&inputs, platform, &mut *console, scanner)
    }))
    .unwrap_or_else(|_| phases::Outcome::stopped(inputs.source.is_some()));
    let (verdict, failed) = (outcome.verdict, outcome.failed);
    let written = catch_unwind(AssertUnwindSafe(|| {
        output::publish(outcome, &out, &mut *console, scanner)
    }));
    drop(source);
    let mut say = Console::new(&mut *console, scanner.clone());
    if verdict.failed() {
        say.verdict(verdict);
    }
    let cleaned = root.remove_root(platform.recycler());
    if let Err(error) = cleaned {
        say.guard_error(output::CLEANUP_FAILED, error);
    }
    if verdict.failed() {
        Exit::DataSafety
    } else if !failed && matches!(written, Ok(true)) && cleaned.is_ok() {
        Exit::Done
    } else {
        Exit::Failed
    }
}
