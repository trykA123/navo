mod cache;
mod diagnostics;
mod errors;
mod listing;
mod location;
mod reading;
mod tree;

#[cfg(test)]
mod tests;

pub use diagnostics::GitProblem;
pub use location::location;

use std::path::Path;
use std::sync::Arc;

use rusty_core::{FsError, FsErrorKind, Location, ProviderId};
use rusty_git::{
    BlobReader, CliGit, FilterNotice, GitError, GitSource, ObjectId, TreeEntry, TreeKind,
};

use crate::{
    CancelToken, Capabilities, CopyControl, FileIdentity, FsEntry, ListSink, OwnedCopy, Provider,
    ProviderOps,
};

use cache::{Cache, Open, TreeLoad};
use location::{GitLocation, ParseError};

pub const READ_ONLY: &str = "Git refs are read-only";

pub struct GitFs {
    factory: Arc<Open>,
    cache: Cache,
    problems: diagnostics::Problems,
}

struct Resolved {
    source: Arc<dyn GitSource>,
    commit: ObjectId,
}

impl GitFs {
    #[must_use]
    pub fn new() -> Self {
        Self::with_factory(|repo| Ok(Box::new(CliGit::new(repo)?)))
    }

    fn with_factory<F>(factory: F) -> Self
    where
        F: Fn(&Path) -> Result<Box<dyn GitSource>, GitError> + Send + Sync + 'static,
    {
        let factory: Arc<Open> = Arc::new(factory);
        Self {
            factory,
            cache: Cache::default(),
            problems: diagnostics::Problems::default(),
        }
    }

    pub fn refresh(&self) {
        self.cache.refresh();
        self.problems.clear_all();
    }

    #[must_use]
    pub fn last_problem(&self, location: &Location) -> Option<GitProblem> {
        self.problems.last(location)
    }

    pub fn tree_entry(&self, location: &Location) -> Result<TreeEntry, FsError> {
        let (_, entry) = reading::open_entry(self, location)?;
        if entry.kind == TreeKind::Submodule {
            return Err(errors::unsupported(location));
        }
        Ok(entry)
    }

    pub fn object_id(&self, location: &Location) -> Result<ObjectId, FsError> {
        self.tree_entry(location).map(|entry| entry.id)
    }

    pub fn read(&self, location: &Location) -> Result<BlobReader, FsError> {
        reading::read(self, location)
    }

    pub(super) fn record_filter(&self, location: &Location, notice: FilterNotice) {
        self.problems.record(location, GitProblem::Filter(notice));
    }

    pub(super) fn fail(&self, location: &Location, error: GitError) -> FsError {
        self.problems
            .record(location, GitProblem::Git(error.clone()));
        errors::git(error, location)
    }

    fn refuse(&self, location: &Location) -> FsError {
        self.problems.record(location, GitProblem::ReadOnly);
        errors::read_only(location)
    }

    fn parse(&self, location: &Location) -> Result<GitLocation, FsError> {
        match location::parse(location) {
            Ok(found) => {
                self.problems.clear(location);
                Ok(found)
            }
            Err(error) => Err(self.parse_failure(location, error)),
        }
    }

    fn parse_failure(&self, location: &Location, error: ParseError) -> FsError {
        let (problem, kind) = match error {
            ParseError::WrongProvider | ParseError::MissingSeparator | ParseError::EmptyRev => {
                (GitProblem::BadLocation, FsErrorKind::NotFound)
            }
            ParseError::BadEscape | ParseError::InvalidUtf8 => {
                (GitProblem::BadLocation, FsErrorKind::NameInvalid)
            }
            ParseError::TooLong => (
                GitProblem::LocationTooLong {
                    limit: location::MAX_LOCATION_BYTES,
                },
                FsErrorKind::PathTooLong,
            ),
            ParseError::TooManyComponents => (
                GitProblem::TooManyComponents {
                    limit: location::MAX_COMPONENTS,
                },
                FsErrorKind::PathTooLong,
            ),
        };
        self.problems.record(location, problem);
        errors::at(kind, location)
    }

    fn resolved(&self, found: &GitLocation) -> Result<Resolved, FsError> {
        let source = self
            .cache
            .source(&found.repo, self.factory.as_ref())
            .map_err(|error| self.fail(&found.location, error))?;
        let Some(commit) = self
            .cache
            .resolve(&found.repo, &found.rev, source.as_ref())
            .map_err(|error| self.fail(&found.location, error))?
        else {
            self.problems.record(
                &found.location,
                GitProblem::RevNotFound {
                    rev: found.rev.clone(),
                },
            );
            return Err(errors::not_found(&found.location));
        };
        Ok(Resolved { source, commit })
    }

    fn tree(&self, found: &GitLocation) -> Result<Arc<Vec<TreeEntry>>, FsError> {
        let resolved = self.resolved(found)?;
        match self
            .cache
            .tree(&found.repo, resolved.commit, resolved.source.as_ref())
        {
            Ok(TreeLoad::Loaded(tree)) => Ok(tree),
            Ok(TreeLoad::TooLarge(size)) => {
                self.problems.record(
                    &found.location,
                    GitProblem::TreeTooLarge {
                        found: size,
                        limit: cache::MAX_TREE_ENTRIES,
                    },
                );
                Err(errors::unsupported(&found.location))
            }
            Err(error) => Err(self.fail(&found.location, error)),
        }
    }
}

impl Default for GitFs {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub fn provider_id() -> ProviderId {
    match ProviderId::new("git") {
        Ok(id) => id,
        Err(_) => ProviderId::local(),
    }
}

impl Provider for GitFs {
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

impl ProviderOps for GitFs {
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

    fn link_target(&self, at: &Location) -> Result<Option<Location>, FsError> {
        reading::link_target(self, at)
    }
}
