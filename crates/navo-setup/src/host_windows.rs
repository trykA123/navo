//! Windows Host implementation over the chain probe, Pin, payload, upgrade and scan.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child as ProcessChild, Command, Stdio};
use std::time::{Duration, Instant};

use navo_core::{FileIdentity, FileIdentityKind};

use crate::host::Host;
use crate::host::ShortcutReport;
use crate::marker::Marker;
use crate::mode::Discovery;
use crate::payload::{Extracted as PlatformExtracted, Payload, Work};
use crate::placement::{self, Chain, FileId, Refused, Space};
use crate::probe::{chain, discovery, facts, pin};
use crate::runner::{Child, Cleanup, Extracted, MarkerId, Pin, Registry, Verified};
use crate::syntax::parse_candidate;
use crate::upgrade::{self, Staged};

#[path = "host_checks.rs"]
mod checks;
#[path = "host_files.rs"]
mod files;
use checks::{folder_error, refuse_existing, refuse_existing_folders, refuse_fresh};
use files::{hash_reader, marker_id, open_regular};

/// The extracted Setup.exe and work folder; `setup` drops first so a bare close can delete it.
struct FreshExtraction {
    setup: Option<PlatformExtracted>,
    work: Option<Work>,
}

/// One held root plus the chain and refused snapshot its rechecks compare with.
struct Held {
    pin: pin::Pin,
    chain: Chain,
    refused: Refused,
    root: PathBuf,
    identity: FileId,
    fresh: bool,
}

/// The real Host: each call reaches the Windows probe that owns that step.
#[derive(Debug, Default)]
pub struct WindowsHost;

impl Host for WindowsHost {
    /// Reads `TokenElevation`; any error is reported so the runner refuses (IW13).
    fn elevated(&mut self) -> io::Result<bool> {
        facts::elevated()
    }

    /// Discovers the registry and legacy installs through the Windows probe.
    fn discover(&mut self) -> io::Result<Discovery> {
        discovery::discover()
    }

    /// Creates and holds the fresh chain after placement; every post-create error removes it.
    fn pin_fresh(&mut self, root: &Path, space: &Space) -> io::Result<Pin> {
        let chain = resolved(root)?;
        refuse_fresh(&chain)?;
        let refused = chain::refused()?;
        let facts = facts::facts(&chain)?;
        placement::check(&chain, &refused, &facts, space, &chain::OrdinalNames)
            .map_err(folder_error)?;
        let held = pin::Pin::fresh(&chain).map_err(|error| match error {
            pin::PinError::RootExists => io::Error::from(io::ErrorKind::AlreadyExists),
            pin::PinError::Io(error) => error,
            other => io::Error::other(other.to_string()),
        })?;
        let after = match resolved(root) {
            Ok(after) => after,
            Err(error) => {
                let _ = held.remove_created();
                return Err(error);
            }
        };
        let identity = match after.existing.last() {
            Some(link) => link.id,
            None => {
                let _ = held.remove_created();
                return Err(io::Error::other("held root has no file id"));
            }
        };
        Ok(Pin::new(Held {
            pin: held,
            chain,
            refused,
            root: root.to_path_buf(),
            identity,
            fresh: true,
        }))
    }

    /// Holds an existing root after rechecking its reparse flags, refused folders and identity.
    fn pin_existing(&mut self, root: &Path, expected: FileIdentity) -> io::Result<Pin> {
        let chain = resolved(root)?;
        refuse_existing(&chain)?;
        let refused = chain::refused()?;
        refuse_existing_folders(&chain, &refused, false)?;
        let identity = chain
            .existing
            .last()
            .ok_or_else(|| io::Error::other("existing root has no ancestor"))?
            .id;
        if !chain.missing.is_empty() || placement_id(expected)? != identity {
            return Err(io::Error::other(
                "existing root identity differs from discovery",
            ));
        }
        let held = pin::Pin::existing(root, identity)
            .map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Pin::new(Held {
            pin: held,
            chain,
            refused,
            root: root.to_path_buf(),
            identity,
            fresh: false,
        }))
    }

    /// Rechecks held folders, ancestors, the refused set and the root's placement.
    fn ancestors_unchanged(&mut self, pin: &Pin) -> io::Result<bool> {
        let held = held(pin)?;
        let refused = match chain::refused() {
            Ok(refused) => refused,
            Err(_) => return Ok(false),
        };
        if refused != held.refused {
            return Ok(false);
        }
        let current = match resolved(&held.root) {
            Ok(current) => current,
            Err(_) => return Ok(false),
        };
        Ok(held.pin.unchanged()
            && pin::ancestors_unchanged(&held.chain)
            && refuse_existing(&current).is_ok()
            && refuse_existing_folders(&current, &refused, held.fresh).is_ok())
    }

    /// Runs IW8b step 2's probe against the held root.
    fn writable(&mut self, pin: &Pin) -> io::Result<bool> {
        upgrade::writable(&held(pin)?.pin)
    }

    /// Scans for any process whose image is under the root; an error is `Err`.
    fn running(&mut self, root: &Path) -> io::Result<bool> {
        navo_diag::processes::any_under(root)
    }

    /// Reports whether the held root holds only an uninstall leftover; an error counts as not empty.
    fn root_empty(&mut self, pin: &Pin) -> io::Result<bool> {
        let held = held(pin)?;
        let root = held
            .pin
            .root()
            .ok_or_else(|| io::Error::other("pin has no root"))?;
        crate::host::uninstall_leftovers_only(root)
    }

    /// Creates the private work folder, extracts Setup.exe and holds it.
    fn extract(&mut self, payload: &Payload) -> io::Result<Extracted> {
        let work = Work::create()?;
        let extracted = payload.extract(&work)?;
        let path = extracted.path.clone();
        Ok(Extracted::new(
            path,
            FreshExtraction {
                setup: Some(extracted),
                work: Some(work),
            },
        ))
    }

    /// Stages the nupkg through the upgrade module.
    fn stage_package(&mut self, pin: &Pin, payload: &Payload) -> io::Result<Staged> {
        upgrade::stage_package(&held(pin)?.pin, payload)
    }

    /// Replaces Update.exe through the upgrade module.
    fn replace_update_exe(&mut self, pin: &Pin, payload: &Payload) -> io::Result<[u8; 32]> {
        upgrade::replace_update_exe(&held(pin)?.pin, payload)
    }

    /// Reads HKCU and compares its path by file id, and by exact text when fresh.
    fn registry_root(&mut self, pin: &Pin) -> io::Result<Registry> {
        let held = held(pin)?;
        let Some(path) = discovery::registry_root()? else {
            return Ok(Registry::Absent);
        };
        let same = same_root(&path, held);
        Ok(if same {
            Registry::NamesRoot
        } else {
            Registry::NamesOther(path)
        })
    }

    /// Spawns one child with no shell and detached standard streams.
    fn spawn(&mut self, args: &[OsString]) -> io::Result<Box<dyn Child>> {
        let (program, rest) = args
            .split_first()
            .ok_or_else(|| io::Error::other("empty Velopack argument array"))?;
        let child = Command::new(program)
            .args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Box::new(Process(child)))
    }

    /// Serves IW4 checks 2 and 3 from the root.
    fn verify_files(
        &mut self,
        root: &Path,
        update_sha256: Option<[u8; 32]>,
    ) -> io::Result<Verified> {
        let update = root.join("Update.exe");
        let mut update_exe = false;
        if let Some(mut file) = open_regular(&update)? {
            update_exe = match update_sha256 {
                Some(expected) => hash_reader(&mut file)? == expected,
                None => true,
            };
        }
        let sq_version = discovery::read_sq_version(&root.join("current").join("sq.version")).ok();
        Ok(Verified {
            update_exe,
            sq_version,
        })
    }

    /// Writes the marker with `CREATE_NEW`, flushes it, and records its file id.
    fn write_marker(&mut self, pin: &Pin, marker: &Marker) -> io::Result<MarkerId> {
        let held = held(pin)?;
        let text = marker
            .render()
            .map_err(|error| io::Error::other(error.to_string()))?;
        let path = held.root.join("navo-install.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        marker_id(&file)
    }

    /// Reads the marker through the bounded parser; absent is `None`.
    fn read_marker(&mut self, root: &Path) -> io::Result<Option<Marker>> {
        discovery::read_marker(&root.join("navo-install.json"))
    }

    /// Removes only the exact unticked links after a successful fresh install.
    fn remove_shortcuts(&mut self, root: &Path, desktop: bool, start_menu: bool) -> ShortcutReport {
        crate::shortcuts::remove_unticked(root, desktop, start_menu)
    }

    /// Closes the work folder and held handles; only a fresh failure removes folders.
    fn cleanup(
        &mut self,
        pin: Pin,
        extracted: Option<Extracted>,
        remove_created: bool,
    ) -> io::Result<Cleanup> {
        if let Some(extracted) = extracted {
            let FreshExtraction {
                mut setup,
                mut work,
            } = extracted
                .take::<FreshExtraction>()
                .map_err(|_| io::Error::other("extraction state mismatch"))?;
            if let (Some(work), Some(setup)) = (work.take(), setup.take()) {
                let _ = work.cleanup(Some(setup));
            }
        }
        let held = pin
            .take::<Held>()
            .map_err(|_| io::Error::other("pin state mismatch"))?;
        let kept = if remove_created {
            held.pin.remove_created().kept
        } else {
            Vec::new()
        };
        Ok(Cleanup { kept })
    }
}

/// Parses and resolves one root path into its existing ancestors and missing names.
fn resolved(root: &Path) -> io::Result<Chain> {
    let candidate = parse_candidate(root.as_os_str())
        .map_err(|_| io::Error::other("install root fails IW6 syntax"))?;
    chain::resolve_chain(&candidate)
}

/// Borrows the platform hold state from a runner pin.
fn held(pin: &Pin) -> io::Result<&Held> {
    pin.get::<Held>()
        .ok_or_else(|| io::Error::other("pin is not a Windows host pin"))
}

/// Converts discovery's native identity back into the probe's file id.
fn placement_id(identity: FileIdentity) -> io::Result<FileId> {
    if identity.kind != FileIdentityKind::Native {
        return Err(io::Error::other("install identity is not native"));
    }
    Ok(FileId {
        volume: identity.volume,
        id: u128::from_le_bytes(identity.file),
    })
}

/// Whether HKCU's path resolves to the held root; fresh also requires the exact text.
fn same_root(path: &Path, held: &Held) -> bool {
    if held.fresh && path != held.root.as_path() {
        return false;
    }
    match chain::resolve_path(path) {
        Ok(chain) => {
            chain.missing.is_empty()
                && chain
                    .existing
                    .last()
                    .is_some_and(|link| link.id == held.identity)
        }
        Err(_) => false,
    }
}

/// The one Velopack child, polled at 25 ms so IW4's progress events stay close to seconds.
struct Process(ProcessChild);

impl Child for Process {
    /// Polls until the deadline; `None` means the child still runs.
    fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Option<i32>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Ok(Some(status.code().unwrap_or(-1)));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            std::thread::sleep(
                deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(25)),
            );
        }
    }

    /// Waits without a deadline and returns the exit code.
    fn wait(&mut self) -> io::Result<i32> {
        Ok(self.0.wait()?.code().unwrap_or(-1))
    }
}
