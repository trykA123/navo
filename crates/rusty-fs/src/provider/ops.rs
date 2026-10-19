use rusty_core::{FsError, Location};

use super::{CancelToken, Provider};

pub trait ProviderOps: Provider {
    fn create_folder(&self, location: &Location) -> Result<(), FsError>;
    fn create_file(&self, location: &Location) -> Result<(), FsError>;
    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P8 O5 contract"
    )]
    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError>;
    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError>;
    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError>;
    fn remove_file(&self, location: &Location) -> Result<(), FsError>;
    fn remove_link(&self, location: &Location) -> Result<(), FsError>;
    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError>;
    fn recycle(&self, location: &Location) -> Result<(), FsError>;
    fn volume_serial(&self, _location: &Location) -> Result<Option<u64>, FsError> {
        Ok(None)
    }
}
