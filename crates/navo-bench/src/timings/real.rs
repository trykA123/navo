use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::error::TimingError;
use super::host::{Capture, Host, Launch, Subject, Window};
use super::layout;
use crate::guard::ScratchRoot;
use crate::probe::{Arg, CloseOutcome, Key, Launched, PathFlag, verify_outputs};
use crate::source::ReadOnlySource;

/// A 100k folder on a slow laptop disk still lists in far less than this.
const BENCH_TIMEOUT: Duration = Duration::from_secs(600);

/// The real machine: the tester exe, a guarded run root and the owner's folder, if any.
/// The folder arrives as an opened `ReadOnlySource`, so it has passed the source policy;
/// every launch checks it again.
pub struct RealHost<'a> {
    root: &'a ScratchRoot,
    exe: PathBuf,
    owner: Option<ReadOnlySource>,
    began: Instant,
}

impl<'a> RealHost<'a> {
    pub fn new(root: &'a ScratchRoot, exe: PathBuf, owner: Option<ReadOnlySource>) -> Self {
        Self {
            root,
            exe,
            owner,
            began: Instant::now(),
        }
    }

    /// The folder argument for `flag`: read-only for the owner's, in the run root otherwise.
    fn folder(&self, subject: Subject, flag: PathFlag) -> Result<Arg<'_>, TimingError> {
        match subject {
            Subject::Owner => {
                let owner = self.owner.as_ref().ok_or(TimingError::NoFolder)?;
                Ok(Arg::ReadOnly(flag, owner.folder()))
            }
            Subject::Generated => Ok(Arg::Path(flag, Path::new(layout::GENERATED))),
        }
    }

    fn read_text(&self, rel: &Path) -> Result<String, TimingError> {
        verify_outputs(self.root, &[rel])?;
        let full = self.root.path().join(rel);
        self.root.check(&full)?;
        fs::read_to_string(full).map_err(|error| TimingError::Io(error.kind()))
    }
}

impl Host for RealHost<'_> {
    fn prepare(&mut self) -> Result<(), TimingError> {
        layout::build(self.root)
    }

    fn generate(&mut self, files: u32) -> Result<(), TimingError> {
        layout::generate(self.root, files)
    }

    fn run_bench(&mut self, subject: Subject, run: u32) -> Result<String, TimingError> {
        let out = layout::bench_out(run);
        let args = [
            Arg::Bench,
            self.folder(subject, PathFlag::Folder)?,
            Arg::Path(PathFlag::Out, &out),
        ];
        let mut process = Launched::start(&self.exe, self.root, &args)?;
        match process.wait_exit(BENCH_TIMEOUT)? {
            None => Err(TimingError::BenchTimeout),
            Some(0) => self.read_text(&out),
            Some(_) => Err(TimingError::BenchExit),
        }
    }

    fn launch(&mut self, launch: Launch) -> Result<Box<dyn Window>, TimingError> {
        let left = self.folder(launch.subject, PathFlag::Left)?;
        let profile = layout::profile(launch.run);
        self.root.create_dir(&profile)?;
        let (metrics, trace) = (layout::metrics(launch.run), layout::trace(launch.run));
        let mut args = vec![Arg::Path(PathFlag::Profile, &profile), left];
        if launch.metrics {
            args.push(Arg::Path(PathFlag::Metrics, &metrics));
        }
        if launch.trace {
            args.push(Arg::Path(PathFlag::Trace, &trace));
        }
        Ok(Box::new(Launched::start(&self.exe, self.root, &args)?))
    }

    fn read(&mut self, capture: Capture, run: u32) -> Option<String> {
        let rel = match capture {
            Capture::Metrics => layout::metrics(run),
            Capture::Trace => layout::trace(run),
        };
        self.read_text(&rel).ok()
    }

    fn pause(&mut self, time: Duration) {
        std::thread::sleep(time);
    }

    fn elapsed(&self) -> Duration {
        self.began.elapsed()
    }
}

impl Window for Launched {
    fn wait_for_window(&mut self, timeout: Duration) -> Result<Duration, TimingError> {
        Launched::wait_for_window(self, timeout).map_err(Into::into)
    }

    fn post_keys(&mut self, keys: &[Key]) -> Result<(), TimingError> {
        Launched::post_keys(self, keys).map_err(Into::into)
    }

    fn close(self: Box<Self>, deadline: Duration) -> Result<CloseOutcome, TimingError> {
        Launched::close(*self, deadline).map_err(Into::into)
    }
}
