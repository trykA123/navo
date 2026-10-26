use std::fs::File;
use std::io;
use std::time::{Duration, Instant};

use rusty_fs::CancelToken;

mod anchor;
mod path;

#[cfg(test)]
pub(crate) fn test_file() -> File {
    path::open().unwrap()
}

#[cfg(test)]
pub(crate) fn test_isolation() -> impl Drop {
    path::Isolation::new()
}

const WAIT: Duration = Duration::from_secs(3);

pub(super) fn claim(cancel: Option<&CancelToken>) -> Result<File, String> {
    acquire(path::open()?, false, cancel, WAIT)
}

pub(super) fn shared(cancel: Option<&CancelToken>) -> Result<File, String> {
    acquire(path::open()?, true, cancel, WAIT)
}

fn acquire(
    file: File,
    shared: bool,
    cancel: Option<&CancelToken>,
    timeout: Duration,
) -> Result<File, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.is_some_and(CancelToken::is_cancelled) {
            return Err(rusty_l10n::tr("job-wait-cancelled"));
        }
        let result = if shared {
            file.try_lock_shared()
        } else {
            file.try_lock()
        };
        match result {
            Ok(()) => return Ok(file),
            Err(error) => {
                let error = io::Error::from(error);
                if error.kind() != io::ErrorKind::WouldBlock {
                    return Err(error.to_string());
                }
            }
        }
        if let Some(cancel) = cancel {
            cancel.wait_cancelled(Duration::from_millis(50));
        } else {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(rusty_l10n::tr("job-profile-busy"));
            }
            std::thread::park_timeout(remaining.min(Duration::from_millis(50)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(path: &std::path::Path) -> File {
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
    }

    #[test]
    fn ordinary_mutations_share_the_lease_and_recovery_has_a_deadline() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lock");
        let first = acquire(open(&path), true, None, WAIT).unwrap();
        let second = acquire(open(&path), true, None, WAIT).unwrap();
        assert!(acquire(open(&path), false, None, Duration::ZERO).is_err());
        drop((first, second));
        assert!(acquire(open(&path), false, None, WAIT).is_ok());
    }

    #[test]
    fn a_retention_wait_can_cancel_before_obtaining_the_lease() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lock");
        let _first = acquire(open(&path), false, None, WAIT).unwrap();
        let cancel = CancelToken::new();
        let worker_cancel = cancel.clone();
        let worker = std::thread::spawn(move || {
            acquire(open(&path), false, Some(&worker_cancel), WAIT).is_err()
        });
        cancel.cancel();
        assert!(worker.join().unwrap());
    }
}
