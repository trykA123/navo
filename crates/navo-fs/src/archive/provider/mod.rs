mod cache;
mod diagnostics;
mod errors;
mod index;
mod listing;
mod location;
mod reading;

pub use diagnostics::{ArchiveProblem, UnsafeReason};
pub use location::{location, raw_location};

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use navo_core::{FsError, FsErrorKind, Location, ProviderId};

use crate::{
    CancelToken, Capabilities, CopyControl, FileIdentity, FsEntry, ListSink, OwnedCopy, Provider,
    ProviderOps, StreamWrite,
};

use super::{ENCRYPTED, NOT_ARCHIVE, UNSUPPORTED_METHOD};
use cache::{Cache, LoadError};
use location::{ArchiveLocation, ParseError};

pub const READ_ONLY: &str = "Archives are read-only";

pub(crate) type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

pub struct ArchiveFs {
    cache: Cache,
    problems: Arc<diagnostics::Problems>,
}

impl ArchiveFs {
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(Instant::now))
    }

    pub(crate) fn with_clock(clock: Clock) -> Self {
        Self {
            cache: Cache::new(clock),
            problems: Arc::new(diagnostics::Problems::default()),
        }
    }

    pub fn refresh(&self) {
        self.cache.refresh();
        self.problems.clear_all();
    }

    #[must_use]
    pub fn last_problem(&self, location: &Location) -> Option<ArchiveProblem> {
        self.problems.last(location)
    }

    pub fn read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        reading::open(self, location)
    }

    pub(in crate::archive::provider) fn parse(
        &self,
        location: &Location,
    ) -> Result<ArchiveLocation, FsError> {
        match location::parse(location) {
            Ok(found) => {
                self.problems.clear(location);
                Ok(found)
            }
            Err(error) => Err(self.parse_failure(location, error)),
        }
    }

    pub(in crate::archive::provider) fn load(
        &self,
        archive: &Path,
        at: &Location,
        cancel: &CancelToken,
    ) -> Result<Arc<index::ArchiveIndex>, FsError> {
        self.cache
            .load(archive, cancel)
            .map_err(|error| self.load_failure(at, error))
    }

    pub(in crate::archive::provider) fn record(
        &self,
        location: &Location,
        problem: ArchiveProblem,
    ) {
        self.problems.record(location, problem);
    }

    fn refuse(&self, location: &Location) -> FsError {
        self.problems.record(location, ArchiveProblem::ReadOnly);
        errors::read_only(location)
    }

    fn parse_failure(&self, location: &Location, error: ParseError) -> FsError {
        let (problem, kind) = match error {
            ParseError::WrongProvider | ParseError::MissingSeparator | ParseError::EmptyArchive => {
                (ArchiveProblem::BadLocation, FsErrorKind::NotFound)
            }
            ParseError::BadEscape | ParseError::InvalidUtf8 => {
                (ArchiveProblem::BadLocation, FsErrorKind::NameInvalid)
            }
            ParseError::TooLong => (
                ArchiveProblem::LocationTooLong {
                    limit: location::MAX_LOCATION_BYTES,
                },
                FsErrorKind::PathTooLong,
            ),
            ParseError::TooManyComponents => (
                ArchiveProblem::TooManyComponents {
                    limit: location::MAX_COMPONENTS,
                },
                FsErrorKind::PathTooLong,
            ),
        };
        self.problems.record(location, problem);
        errors::at(kind, location)
    }

    fn load_failure(&self, location: &Location, error: LoadError) -> FsError {
        match error {
            LoadError::Io(error) => {
                let failed = errors::from_io(&error, location);
                self.problems.record(
                    location,
                    ArchiveProblem::Damaged {
                        detail: error.to_string(),
                    },
                );
                failed
            }
            LoadError::List(message) => self.list_failure(location, message),
            LoadError::Changed => {
                self.problems.record(location, ArchiveProblem::Changed);
                errors::changed(location)
            }
            LoadError::Cancelled => errors::cancelled(location),
        }
    }

    fn list_failure(&self, location: &Location, message: String) -> FsError {
        if message == NOT_ARCHIVE {
            self.problems.record(location, ArchiveProblem::NotAnArchive);
            return errors::unsupported(location);
        }
        if message == ENCRYPTED {
            self.problems.record(
                location,
                ArchiveProblem::Encrypted {
                    name: archive_name(location),
                },
            );
            return errors::unsupported(location);
        }
        if message == UNSUPPORTED_METHOD {
            self.problems
                .record(location, ArchiveProblem::Damaged { detail: message });
            return errors::unsupported(location);
        }
        self.problems
            .record(location, ArchiveProblem::Damaged { detail: message });
        errors::at(FsErrorKind::Unknown, location)
    }
}

impl Default for ArchiveFs {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub fn provider_id() -> ProviderId {
    match ProviderId::new("archive") {
        Ok(id) => id,
        Err(_) => ProviderId::local(),
    }
}

fn archive_name(location: &Location) -> String {
    location::parse(location).ok().map_or_else(
        || location.path().to_text(),
        |found| {
            found
                .archive
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| found.archive.to_string_lossy().into_owned())
        },
    )
}

impl Provider for ArchiveFs {
    fn id(&self) -> ProviderId {
        provider_id()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        listing::list(self, folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        reading::metadata(self, location)
    }
}

impl ProviderOps for ArchiveFs {
    fn hide_retained(&self, location: &Location, _identity: FileIdentity) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn copy_file_owned(
        &self,
        request: OwnedCopy<'_>,
        _control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        Err(self.refuse(request.target))
    }

    fn write_new(
        &self,
        request: StreamWrite<'_>,
        _control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        Err(self.refuse(request.target))
    }

    fn remove_owned_file(
        &self,
        location: &Location,
        _identity: FileIdentity,
    ) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn rename_atomic(&self, source: &Location, _target: &Location) -> Result<(), FsError> {
        Err(self.refuse(source))
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn create_private_folder(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn restrict_to_owner(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn copy_file(
        &self,
        _source: &Location,
        target: &Location,
        _progress: &mut dyn FnMut(u64),
        _cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        Err(self.refuse(target))
    }

    fn rename(&self, source: &Location, _target: &Location) -> Result<(), FsError> {
        Err(self.refuse(source))
    }

    fn replace(&self, source: &Location, _target: &Location) -> Result<(), FsError> {
        Err(self.refuse(source))
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        Err(self.refuse(location))
    }

    fn make_link(&self, _target_of: &Location, at: &Location) -> Result<(), FsError> {
        Err(self.refuse(at))
    }
}
