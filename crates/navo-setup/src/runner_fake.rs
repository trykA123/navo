//! Recording fake Host for the runner tests; every call is kept for assertions.

#![allow(
    dead_code,
    reason = "shared test fake; each test module uses a different subset"
)]

use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use navo_core::FileIdentity;

use crate::cli::CliHost;
use crate::host::Host;
use crate::marker::Marker;
use crate::mode::Discovery;
use crate::payload::Payload;
use crate::placement::FolderWarning;
use crate::runner::{Child, Cleanup, Extracted, MarkerId, Pin, Registry, Verified};
use crate::upgrade::Staged;

/// The fake extracted Setup.exe path used in fresh argument arrays.
pub const SETUP_PATH: &str = r"C:\Temp\NavoSetup-test\Setup.exe";

/// One recorded Host call, including the arrays and values the runner passed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    Elevated,
    Discover,
    PinFresh(PathBuf),
    PinExisting(PathBuf, FileIdentity),
    Ancestors,
    Writable,
    Running(PathBuf),
    RootEmpty,
    Extract,
    Stage,
    Replace,
    Registry,
    Spawn(Vec<OsString>),
    Verify(Option<[u8; 32]>),
    WriteMarker(Marker),
    ReadMarker(PathBuf),
    RemoveShortcuts {
        root: PathBuf,
        desktop: bool,
        start_menu: bool,
    },
    Cleanup {
        remove_created: bool,
    },
}

/// What a scripted marker write does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerWrite {
    Ok,
    Exists,
    Fail,
}

/// A Host whose answers are scripted per call and whose calls are recorded.
pub struct FakeHost {
    pub calls: Vec<Call>,
    pub elevated: VecDeque<io::Result<bool>>,
    pub discoveries: VecDeque<io::Result<Discovery>>,
    pub ancestors: VecDeque<bool>,
    pub held_changed: bool,
    pub held_changed_after_spawn: bool,
    pub spawned: bool,
    pub writable: VecDeque<bool>,
    pub running: VecDeque<io::Result<bool>>,
    pub parent_warning: Option<FolderWarning>,
    pub extract_ok: bool,
    pub stage_ok: bool,
    pub replace_ok: bool,
    pub pin_existing_ok: bool,
    pub pin_fresh_ok: bool,
    pub pin_fresh_problem: Option<crate::syntax::FolderProblem>,
    pub pin_fresh_space: Option<crate::placement::Space>,
    pub registry: VecDeque<Registry>,
    pub marker_writes: VecDeque<MarkerWrite>,
    pub markers: VecDeque<Option<Marker>>,
    pub verified: VecDeque<Verified>,
    pub child_script: VecDeque<Option<i32>>,
    pub child_wait_error: bool,
    pub child_wait_code: i32,
    pub wait_calls: Rc<Cell<usize>>,
    pub cleanup_wait_calls: Rc<Cell<Option<usize>>>,
    pub root_empty: VecDeque<io::Result<bool>>,
    pub kept: Vec<PathBuf>,
}

impl FakeHost {
    /// Builds a host with no scripted answers; each call uses a safe default.
    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
            elevated: VecDeque::new(),
            discoveries: VecDeque::new(),
            ancestors: VecDeque::new(),
            held_changed: false,
            held_changed_after_spawn: false,
            spawned: false,
            writable: VecDeque::new(),
            running: VecDeque::new(),
            parent_warning: None,
            extract_ok: true,
            stage_ok: true,
            replace_ok: true,
            pin_existing_ok: true,
            pin_fresh_ok: true,
            pin_fresh_problem: None,
            pin_fresh_space: None,
            registry: VecDeque::new(),
            marker_writes: VecDeque::new(),
            markers: VecDeque::new(),
            verified: VecDeque::new(),
            child_script: VecDeque::new(),
            child_wait_error: false,
            child_wait_code: 0,
            wait_calls: Rc::new(Cell::new(0)),
            cleanup_wait_calls: Rc::new(Cell::new(None)),
            root_empty: VecDeque::new(),
            kept: Vec::new(),
        }
    }

    /// Counts calls of one kind by predicate.
    pub fn count(&self, predicate: impl Fn(&Call) -> bool) -> usize {
        self.calls.iter().filter(|call| predicate(call)).count()
    }

    /// Returns every spawned argument array in call order.
    pub fn spawns(&self) -> Vec<&Vec<OsString>> {
        self.calls
            .iter()
            .filter_map(|call| match call {
                Call::Spawn(args) => Some(args),
                _ => None,
            })
            .collect()
    }
}

impl Default for FakeHost {
    /// Starts from the same safe defaults as `new`.
    fn default() -> Self {
        Self::new()
    }
}

/// Pops a scripted answer or returns the default.
fn next<T>(queue: &mut VecDeque<T>, default: T) -> T {
    queue.pop_front().unwrap_or(default)
}

impl CliHost for FakeHost {
    /// Returns the scripted parent warning.
    fn parent_warning(&mut self, _root: &Path) -> Option<FolderWarning> {
        self.parent_warning
    }
}

impl Host for FakeHost {
    /// Records the elevation check and returns the scripted answer.
    fn elevated(&mut self) -> io::Result<bool> {
        self.calls.push(Call::Elevated);
        next(&mut self.elevated, Ok(false))
    }

    /// Records discovery and returns the scripted installs.
    fn discover(&mut self) -> io::Result<Discovery> {
        self.calls.push(Call::Discover);
        next(&mut self.discoveries, Ok(Discovery::default()))
    }

    /// Records a fresh hold and returns an opaque pin.
    fn pin_fresh(&mut self, root: &Path, space: &crate::placement::Space) -> io::Result<Pin> {
        self.calls.push(Call::PinFresh(root.to_path_buf()));
        self.pin_fresh_space = Some(*space);
        if let Some(problem) = self.pin_fresh_problem {
            return Err(crate::host::folder_error(problem));
        }
        if self.pin_fresh_ok {
            Ok(Pin::new(FakeHold {
                root: root.to_path_buf(),
            }))
        } else {
            Err(io::Error::other("placement refused the folder"))
        }
    }

    /// Records an existing hold and returns an opaque pin.
    fn pin_existing(&mut self, root: &Path, expected: FileIdentity) -> io::Result<Pin> {
        self.calls
            .push(Call::PinExisting(root.to_path_buf(), expected));
        if self.pin_existing_ok {
            Ok(Pin::new(FakeHold {
                root: root.to_path_buf(),
            }))
        } else {
            Err(io::Error::other("root identity differs from discovery"))
        }
    }

    /// Records a chain recheck and returns the scripted result.
    fn ancestors_unchanged(&mut self, _pin: &Pin) -> io::Result<bool> {
        self.calls.push(Call::Ancestors);
        let after_spawn = self.held_changed_after_spawn && self.spawned;
        Ok(!self.held_changed && !after_spawn && next(&mut self.ancestors, true))
    }

    /// Records the write probe and returns the scripted result.
    fn writable(&mut self, _pin: &Pin) -> io::Result<bool> {
        self.calls.push(Call::Writable);
        Ok(next(&mut self.writable, true))
    }

    /// Records a process scan and returns the scripted result.
    fn running(&mut self, root: &Path) -> io::Result<bool> {
        self.calls.push(Call::Running(root.to_path_buf()));
        next(&mut self.running, Ok(false))
    }

    /// Records the root emptiness check and returns the scripted answer.
    fn root_empty(&mut self, _pin: &Pin) -> io::Result<bool> {
        self.calls.push(Call::RootEmpty);
        next(&mut self.root_empty, Ok(false))
    }

    /// Records extraction and returns a fake held Setup.exe path.
    fn extract(&mut self, _payload: &Payload) -> io::Result<Extracted> {
        self.calls.push(Call::Extract);
        if self.extract_ok {
            Ok(Extracted::new(PathBuf::from(SETUP_PATH), FakeExtraction))
        } else {
            Err(io::Error::other("extract failed"))
        }
    }

    /// Records package staging and returns the package path under the root.
    fn stage_package(&mut self, pin: &Pin, payload: &Payload) -> io::Result<Staged> {
        self.calls.push(Call::Stage);
        if !self.stage_ok {
            return Err(io::Error::other("stage failed"));
        }
        let root = pin
            .get::<FakeHold>()
            .map(|held| held.root.clone())
            .ok_or_else(|| io::Error::other("fake pin missing"))?;
        Ok(Staged {
            path: root.join("packages").join(&payload.package_name),
        })
    }

    /// Records Update.exe replacement and returns a fixed SHA-256.
    fn replace_update_exe(&mut self, _pin: &Pin, _payload: &Payload) -> io::Result<[u8; 32]> {
        self.calls.push(Call::Replace);
        if self.replace_ok {
            Ok([7; 32])
        } else {
            Err(io::Error::other("replace failed"))
        }
    }

    /// Records the registry check and returns the scripted listing.
    fn registry_root(&mut self, _pin: &Pin) -> io::Result<Registry> {
        self.calls.push(Call::Registry);
        Ok(next(&mut self.registry, Registry::NamesRoot))
    }

    /// Records a spawn and returns a child scripted with timeout steps.
    fn spawn(&mut self, args: &[OsString]) -> io::Result<Box<dyn Child>> {
        self.calls.push(Call::Spawn(args.to_vec()));
        self.spawned = true;
        Ok(Box::new(FakeChild {
            script: self.child_script.clone(),
            last: 0,
            wait_error: self.child_wait_error,
            wait_code: self.child_wait_code,
            wait_calls: Rc::clone(&self.wait_calls),
        }))
    }

    /// Records file verification and returns the scripted checks.
    fn verify_files(
        &mut self,
        _root: &Path,
        update_sha256: Option<[u8; 32]>,
    ) -> io::Result<Verified> {
        self.calls.push(Call::Verify(update_sha256));
        Ok(next(
            &mut self.verified,
            Verified {
                update_exe: true,
                sq_version: None,
            },
        ))
    }

    /// Records a marker write and returns the scripted outcome.
    fn write_marker(&mut self, _pin: &Pin, marker: &Marker) -> io::Result<MarkerId> {
        self.calls.push(Call::WriteMarker(marker.clone()));
        match next(&mut self.marker_writes, MarkerWrite::Ok) {
            MarkerWrite::Ok => Ok(MarkerId {
                volume: 1,
                id: [9; 16],
            }),
            MarkerWrite::Exists => Err(io::Error::from(io::ErrorKind::AlreadyExists)),
            MarkerWrite::Fail => Err(io::Error::other("marker write failed")),
        }
    }

    /// Records a marker read and returns the scripted marker.
    fn read_marker(&mut self, root: &Path) -> io::Result<Option<Marker>> {
        self.calls.push(Call::ReadMarker(root.to_path_buf()));
        Ok(next(&mut self.markers, None))
    }

    /// Records the post-install shortcut pass and returns an empty report.
    fn remove_shortcuts(
        &mut self,
        root: &Path,
        desktop: bool,
        start_menu: bool,
    ) -> crate::host::ShortcutReport {
        self.calls.push(Call::RemoveShortcuts {
            root: root.to_path_buf(),
            desktop,
            start_menu,
        });
        crate::host::ShortcutReport::default()
    }

    /// Records cleanup and returns the scripted kept folders.
    fn cleanup(
        &mut self,
        _pin: Pin,
        _extracted: Option<Extracted>,
        remove_created: bool,
    ) -> io::Result<Cleanup> {
        self.calls.push(Call::Cleanup { remove_created });
        self.cleanup_wait_calls.set(Some(self.wait_calls.get()));
        Ok(Cleanup {
            kept: self.kept.clone(),
        })
    }
}

/// The fake held root; package staging reads its path from here.
struct FakeHold {
    root: PathBuf,
}

/// The fake extraction state; the runner treats it as opaque.
struct FakeExtraction;

/// A scripted child: `None` means one timeout second, `Some(code)` means exit.
struct FakeChild {
    script: VecDeque<Option<i32>>,
    last: i32,
    wait_error: bool,
    wait_code: i32,
    wait_calls: Rc<Cell<usize>>,
}

impl Child for FakeChild {
    /// Pops one scripted step; an exhausted script fails the test instead of looping.
    fn wait_timeout(&mut self, _timeout: Duration) -> io::Result<Option<i32>> {
        if self.wait_error {
            self.wait_error = false;
            return Err(io::Error::other("scripted wait_timeout error"));
        }
        match self.script.pop_front() {
            Some(Some(code)) => {
                self.last = code;
                Ok(Some(code))
            }
            Some(None) => Ok(None),
            None => panic!("FakeChild script exhausted"),
        }
    }

    /// Records the unbounded wait and returns its scripted code.
    fn wait(&mut self) -> io::Result<i32> {
        self.wait_calls.set(self.wait_calls.get() + 1);
        Ok(self.wait_code)
    }
}
