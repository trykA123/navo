use rusty_core::retention::CopyTemporary;
use rusty_core::{FsErrorKind, Location};
use rusty_fs::{EntryKind, FileIdentity, ProviderOps};

pub(super) fn parent(ops: &dyn ProviderOps, proof: &CopyTemporary) -> Result<(), String> {
    let parent = proof
        .temporary
        .parent()
        .ok_or("temporary parent is unavailable")?;
    if ops.metadata(&parent).map_err(|error| error.message())?.kind != EntryKind::Folder
        || ops
            .file_identity(&parent)
            .map_err(|error| error.message())?
            != Some(proof.parent_identity)
        || ops
            .mount_identity(&parent)
            .map_err(|error| error.message())?
            != Some(proof.mount_identity)
    {
        return Err(format!(
            "copy temporary parent changed at {}",
            parent.as_str()
        ));
    }
    Ok(())
}

pub(super) fn identity(
    ops: &dyn ProviderOps,
    location: &Location,
) -> Result<Option<FileIdentity>, String> {
    match ops.file_identity(location) {
        Ok(Some(identity)) => Ok(Some(identity)),
        Ok(None) => Err(format!(
            "copy temporary identity is unavailable at {}",
            location.as_str()
        )),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.message()),
    }
}

pub(super) fn present(ops: &dyn ProviderOps, proof: &CopyTemporary) -> Result<(), String> {
    parent(ops, proof)?;
    if proof.identity.is_none()
        || identity(ops, &proof.temporary)? != proof.identity
        || ops
            .metadata(&proof.temporary)
            .map_err(|error| error.message())?
            .kind
            != EntryKind::File
        || ops
            .mount_identity(&proof.temporary)
            .map_err(|error| error.message())?
            != Some(proof.mount_identity)
    {
        return Err(format!(
            "copy temporary identity changed at {}",
            proof.temporary.as_str()
        ));
    }
    Ok(())
}

pub(super) fn absent(ops: &dyn ProviderOps, proof: &CopyTemporary) -> Result<(), String> {
    parent(ops, proof)?;
    if identity(ops, &proof.temporary)?.is_some() {
        return Err(format!(
            "copy temporary remains at {}",
            proof.temporary.as_str()
        ));
    }
    Ok(())
}
