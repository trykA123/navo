//! The model behind one compare tab: its two roots, the folder walk and the updates it receives.
//! It holds no GPUI handle; the tab owns it and drains the updates on the UI thread.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use navo_compare_cli::args::SideSpec;
use sidewise_core::Location;
use sidewise_ui::browser::{CompareHooks, CompareSink, CompareUpdate, FolderSession, Waker};
use sidewise_ui::compare_diff::CompareOptions;

#[cfg(test)]
pub use super::compare_roots::classify;
#[cfg(test)]
pub(crate) use super::compare_roots::location_of;
use super::compare_roots::roots_of;
use crate::screens::large_model::PhaseMeters;
use crate::start::Side;
use crate::start_tabs::StartChoice;

/// What the two roots are: folders walked by the engine, or two files opened as one pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// Two folders; the engine walks them into rows.
    Folders,
    /// Two files; no walk starts and the pair opens at once.
    Files,
}

/// Why a compare did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The side named does not exist or cannot be read.
    Missing(Side),
    /// One side is a folder and the other is a file.
    Mixed,
    /// A side cannot be turned into a local location; the text says why.
    Location(String),
}

impl std::fmt::Display for RunError {
    /// Writes the user-facing English text of the error.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(Side::Left) => formatter.write_str("the left path does not exist"),
            Self::Missing(Side::Right) => formatter.write_str("the right path does not exist"),
            Self::Mixed => formatter.write_str("one path is a folder and the other is a file"),
            Self::Location(text) => write!(formatter, "the path cannot be opened: {text}"),
        }
    }
}

/// One compare tab's run: the roots, the optional folder session and its update channel.
pub struct CompareRun {
    kind: RunKind,
    session: Option<FolderSession>,
    updates: Receiver<CompareUpdate>,
    roots: [Location; 2],
    meters: PhaseMeters,
    options: CompareOptions,
    wake: Waker,
    /// True once the engine sent an update that is not a failure: both sides opened.
    opened: bool,
}

impl CompareRun {
    /// Classifies the roots and starts the walk for two folders; two files start no walk.
    /// `wake` is called after each update is queued; a bad root returns `RunError`.
    pub fn start(
        (left, right): (&SideSpec, &SideSpec),
        choice: &StartChoice,
        hooks: &dyn CompareHooks,
        wake: Waker,
    ) -> Result<Self, RunError> {
        let (kind, roots) = roots_of(left, right)?;
        let mut options = CompareOptions::default();
        options.compare.respect_gitignore = choice.respect_gitignore;
        let mut run = Self {
            kind,
            session: None,
            updates: mpsc::channel().1,
            roots,
            meters: PhaseMeters::default(),
            options,
            wake,
            opened: false,
        };
        run.restart(hooks, false);
        Ok(run)
    }

    /// Starts the folder walk anew (F5): each side opens again, so a Git side re-resolves its
    /// revision while other tabs keep the side they hold. The old walk is cancelled and its
    /// queued updates are dropped. With `recheck` (F5) every file is read again: Recheck all.
    /// Does nothing for two files.
    pub fn restart(&mut self, hooks: &dyn CompareHooks, recheck: bool) {
        if self.kind != RunKind::Folders {
            return;
        }
        self.options.fresh_hashes = recheck;
        let (sender, updates) = mpsc::channel();
        let wake = Arc::clone(&self.wake);
        let sink: CompareSink = Arc::new(move |update| {
            let _ = sender.send(update);
            wake();
        });
        let control = hooks.start((&self.roots[0], &self.roots[1]), &self.options, sink);
        let number = self.session.as_ref().map_or(1, |session| session.run() + 1);
        self.session = Some(FolderSession::new(number, control, self.roots.clone()));
        self.updates = updates;
        self.meters = PhaseMeters::default();
        self.opened = false;
    }

    /// Whether the run reached the point where rows can arrive: two files, or the engine sent
    /// an update other than a failure.
    pub fn opened(&self) -> bool {
        self.kind == RunKind::Files || self.opened
    }

    /// The text of a compare that did not start, as the engine reported it.
    pub fn failure(&self) -> Option<&str> {
        self.session.as_ref()?.failed.as_deref()
    }

    /// Applies every queued update to the folder session; true when one of them changed it.
    pub fn drain(&mut self) -> bool {
        let mut applied = false;
        for update in self.updates.try_iter() {
            self.opened |= !matches!(update, CompareUpdate::Failed(_));
            applied |= self.meters.observe(&update);
            if let Some(session) = self.session.as_mut() {
                applied |= session.apply(update);
            }
        }
        applied
    }

    /// The D5 phase meters of the newest generation.
    pub fn meters(&self) -> &PhaseMeters {
        &self.meters
    }

    /// What the roots are: folders walked by the engine, or two files.
    pub fn kind(&self) -> RunKind {
        self.kind
    }

    /// The folder session of a folder run; `None` for two files.
    pub fn session(&self) -> Option<&FolderSession> {
        self.session.as_ref()
    }

    /// The folder session, mutable for cursor and view changes.
    pub fn session_mut(&mut self) -> Option<&mut FolderSession> {
        self.session.as_mut()
    }

    /// The two roots as local locations, left first.
    pub fn roots(&self) -> &[Location; 2] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use sidewise_host::compare::handle::std_local;
    use sidewise_host::compare::pane::PaneCompare;

    use super::*;
    use crate::screens::host_parts::NoHashes;

    const WAIT: Duration = Duration::from_secs(20);

    /// A unique folder under the system temp root; its files go away when the guard drops.
    struct TempDir(PathBuf);

    impl TempDir {
        /// Creates the folder with `tag`, the process id and a counter in its name.
        fn new(tag: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let name = format!(
                "sidewise-h1-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            );
            let path = std::env::temp_dir().join(name);
            fs::create_dir(&path).expect("create temp dir");
            Self(path)
        }

        /// The folder's path.
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        /// Removes the folder and everything in it.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A `Waker` that counts its calls, with the counter kept for the assertion.
    fn counting_wake() -> (Waker, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let wakes = Arc::clone(&count);
        let wake: Waker = Arc::new(move || {
            wakes.fetch_add(1, Ordering::SeqCst);
        });
        (wake, count)
    }

    /// Two folders, two files, missing sides and a mixed pair each classify as the brief says.
    #[test]
    fn classify_reports_folders_files_and_bad_pairs() {
        let left = TempDir::new("classify-left");
        let right = TempDir::new("classify-right");
        assert_eq!(classify(left.path(), right.path()), Ok(RunKind::Folders));

        let one = left.path().join("one.txt");
        let two = right.path().join("two.txt");
        fs::write(&one, b"one").expect("write left file");
        fs::write(&two, b"two").expect("write right file");
        assert_eq!(classify(&one, &two), Ok(RunKind::Files));

        let missing = right.path().join("gone");
        assert_eq!(
            classify(&missing, right.path()),
            Err(RunError::Missing(Side::Left))
        );
        assert_eq!(
            classify(left.path(), &missing),
            Err(RunError::Missing(Side::Right))
        );
        assert_eq!(classify(left.path(), &one), Err(RunError::Mixed));
    }

    /// A root whose parent denies access is a location error, not a missing root.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_side_is_a_location_error() {
        use std::os::unix::fs::PermissionsExt as _;

        let parent = TempDir::new("unreadable");
        let child = parent.path().join("inside");
        fs::create_dir(&child).expect("create inside folder");
        fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o000))
            .expect("lock the parent folder");
        let result = classify(&child, &child);
        let readable = fs::metadata(&child).is_ok();
        fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o755))
            .expect("unlock the parent folder");
        if readable {
            return; // root ignores the mode bits, so the case cannot be tested
        }
        assert!(matches!(result, Err(RunError::Location(_))), "{result:?}");
    }

    /// Every `RunError` writes the English text the tab's toast shows.
    #[test]
    fn run_error_display_is_english() {
        assert_eq!(
            RunError::Missing(Side::Left).to_string(),
            "the left path does not exist"
        );
        assert_eq!(
            RunError::Missing(Side::Right).to_string(),
            "the right path does not exist"
        );
        assert_eq!(
            RunError::Mixed.to_string(),
            "one path is a folder and the other is a file"
        );
        assert_eq!(
            RunError::Location("bad".to_owned()).to_string(),
            "the path cannot be opened: bad"
        );
    }

    /// A folder start walks both trees, wakes after updates and shows the root rows.
    #[test]
    fn folders_start_a_walk_and_drain_reports_the_rows() {
        let left = TempDir::new("walk-left");
        let right = TempDir::new("walk-right");
        for root in [left.path(), right.path()] {
            fs::write(root.join("same.txt"), b"one").expect("write same file");
            fs::create_dir(root.join("sub")).expect("create sub folder");
        }
        let (wake, wakes) = counting_wake();
        let mut run = CompareRun::start(
            (
                &SideSpec::Path(left.path().to_path_buf()),
                &SideSpec::Path(right.path().to_path_buf()),
            ),
            &StartChoice::default(),
            &PaneCompare::new(Arc::new(NoHashes), std_local()),
            wake,
        )
        .expect("start folders");

        let deadline = Instant::now() + WAIT;
        let mut changed = false;
        loop {
            changed |= run.drain();
            if run.session().is_some_and(|session| !session.checking()) {
                break;
            }
            assert!(Instant::now() < deadline, "the walk did not finish");
            thread::sleep(Duration::from_millis(10));
        }

        let session = run.session_mut().expect("folder session");
        let mut rows: Vec<String> = session
            .visible()
            .iter()
            .map(|row| row.path.clone())
            .collect();
        rows.sort();
        assert_eq!(run.kind(), RunKind::Folders);
        assert_eq!(rows, ["same.txt", "sub"]);
        assert!(changed, "drain reported an applied update");
        assert!(wakes.load(Ordering::SeqCst) > 0, "the sink woke the tab");
    }

    /// Two files classify as a file run: no session starts and no wake fires.
    #[test]
    fn files_start_no_walk() {
        let left = TempDir::new("files-left");
        let right = TempDir::new("files-right");
        let left_file = left.path().join("a.txt");
        let right_file = right.path().join("b.txt");
        fs::write(&left_file, b"a").expect("write left file");
        fs::write(&right_file, b"b").expect("write right file");

        let (wake, wakes) = counting_wake();
        let mut run = CompareRun::start(
            (
                &SideSpec::Path(left_file.clone()),
                &SideSpec::Path(right_file.clone()),
            ),
            &StartChoice::default(),
            &PaneCompare::new(Arc::new(NoHashes), std_local()),
            wake,
        )
        .expect("start files");

        assert_eq!(run.kind(), RunKind::Files);
        assert!(run.session().is_none());
        assert!(!run.drain());
        assert_eq!(wakes.load(Ordering::SeqCst), 0);
        assert_eq!(
            run.roots()[0],
            location_of(&left_file).expect("left location")
        );
    }
}
