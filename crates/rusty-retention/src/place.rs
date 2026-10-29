use rusty_core::brand::{VAULT_FALLBACK_NAMES, VAULT_NAME};
use rusty_core::{FileIdentity, Location, ProviderPath};
use rusty_fs::{EntryKind, ProviderOps, UnixAccess};

use crate::{Error, proof};

pub(super) struct Placed {
    pub vault: Location,
    pub identity: FileIdentity,
}

#[derive(Default)]
pub(super) struct Config {
    pub fallback: Option<Location>,
    pub ceiling: Option<Location>,
}

pub(super) struct Places<'a> {
    pub root: &'a Location,
    pub parent: &'a Location,
    pub fallback: Option<&'a Location>,
    pub user: Option<u32>,
}

pub(super) type Created = Vec<(Location, FileIdentity)>;

pub(super) fn volume_root(start: &Location, same_volume: impl Fn(&Location) -> bool) -> Location {
    let mut root = start.clone();
    while let Some(up) = root.parent().filter(|up| same_volume(up)) {
        root = up;
    }
    root
}

pub(super) fn candidates(places: &Places<'_>, windows: bool) -> Vec<Vec<Location>> {
    let mut found: Vec<Vec<Location>> = child(places.root, VAULT_NAME)
        .map(|vault| vec![vault])
        .into_iter()
        .collect();
    if windows {
        let mut below: Vec<_> =
            std::iter::successors(Some(places.parent.clone()), Location::parent)
                .take_while(|level| !level.same_place(places.root))
                .collect();
        below.reverse();
        found.extend(
            below
                .iter()
                .filter_map(|level| child(level, VAULT_NAME))
                .map(|vault| vec![vault]),
        );
        return found;
    }
    found.extend(places.fallback.map(|vault| vec![vault.clone()]));
    let trash = places
        .user
        .and_then(|user| child(places.root, &format!(".Trash-{user}")));
    if let Some(trash) = trash {
        found.extend(child(&trash, VAULT_FALLBACK_NAMES[1]).map(|vault| vec![trash, vault]));
    }
    found
}

pub(super) fn chains(
    ops: &dyn ProviderOps,
    (parent, scope): (&Location, proof::Scope),
    config: &Config,
) -> Result<Vec<Vec<Location>>, Error> {
    let mounted = ops.volume_root(parent)?;
    let root = volume_root(parent, |up| {
        config
            .ceiling
            .as_ref()
            .is_none_or(|ceiling| within(up, ceiling))
            && match &mounted {
                Some(mounted) => within(up, mounted),
                None => proof::scope(ops, up).is_ok_and(|found| found == scope),
            }
    });
    let places = Places {
        root: &root,
        parent,
        fallback: config.fallback.as_ref(),
        user: user_id(),
    };
    Ok(candidates(&places, cfg!(windows)))
}

pub(super) fn ensure_chain(
    ops: &dyn ProviderOps,
    (chain, scope): (&[Location], proof::Scope),
    created: &mut Created,
) -> Option<Placed> {
    let mut placed = None;
    for folder in chain {
        match ensure(ops, folder, scope, created) {
            Ok(identity) => {
                placed = Some(Placed {
                    vault: folder.clone(),
                    identity,
                });
            }
            Err(error) => {
                tracing::debug!(vault = folder.as_str(), %error, "vault candidate skipped");
                return None;
            }
        }
    }
    placed
}

pub(super) fn discard(ops: &dyn ProviderOps, created: &Created) {
    for (folder, identity) in created.iter().rev() {
        let removed = proof::folder(ops, folder, *identity)
            .and_then(|()| Ok(ops.remove_empty_folder(folder)?));
        if let Err(error) = removed {
            tracing::debug!(vault = folder.as_str(), %error, "unused vault kept");
        }
    }
}

fn ensure(
    ops: &dyn ProviderOps,
    folder: &Location,
    scope: proof::Scope,
    created: &mut Created,
) -> Result<FileIdentity, Error> {
    let parent = folder
        .parent()
        .ok_or_else(|| Error::Unsupported(folder.clone()))?;
    proof::on_scope(ops, &parent, scope)?;
    let mut made = false;
    if proof::present(ops, folder)?.is_none() {
        let result = ops.create_private_folder(folder);
        made = result.is_ok();
        if proof::present(ops, folder)?.is_none() {
            result?;
        }
    }
    if ops.metadata(folder)?.kind != EntryKind::Folder {
        return Err(Error::Changed(folder.clone()));
    }
    proof::on_scope(ops, folder, scope)?;
    let identity = proof::identity(ops, folder)?;
    if made {
        created.push((folder.clone(), identity));
    }
    if !acceptable(ops, folder, made)? {
        return Err(Error::Unsupported(folder.clone()));
    }
    ops.hide_retained(folder, identity)?;
    Ok(identity)
}

fn acceptable(ops: &dyn ProviderOps, folder: &Location, made: bool) -> Result<bool, Error> {
    let access = ops.unix_access(folder)?;
    if !made && !is_ours(ops, folder, access)? {
        return Ok(false);
    }
    let Some(access) = access else {
        return Ok(true);
    };
    let user = user_id();
    if is_private(access, user) {
        return Ok(true);
    }
    if let Err(error) = ops.restrict_to_owner(folder) {
        tracing::debug!(
            vault = folder.as_str(),
            ?error,
            "vault could not be made private"
        );
        return Ok(false);
    }
    Ok(ops
        .unix_access(folder)?
        .is_some_and(|after| is_private(after, user)))
}

fn is_ours(
    ops: &dyn ProviderOps,
    folder: &Location,
    access: Option<UnixAccess>,
) -> Result<bool, Error> {
    if let Some(owned) = ops.owned_by_current_user(folder)? {
        return Ok(owned);
    }
    Ok(access.is_none_or(|access| user_id() == Some(access.owner)))
}

pub(super) fn is_private(access: UnixAccess, user: Option<u32>) -> bool {
    user == Some(access.owner) && access.mode & 0o077 == 0
}

fn within(at: &Location, top: &Location) -> bool {
    std::iter::successors(Some(at.clone()), Location::parent).any(|level| level.same_place(top))
}

fn child(parent: &Location, name: &str) -> Option<Location> {
    parent.join(&ProviderPath::from_text(name)).ok()
}

#[cfg(unix)]
pub(super) fn user_id() -> Option<u32> {
    Some(rustix::process::getuid().as_raw())
}

#[cfg(not(unix))]
pub(super) fn user_id() -> Option<u32> {
    None
}

pub(super) fn config(profile: &std::path::Path) -> Config {
    Config {
        fallback: profile_fallback(profile),
        ceiling: None,
    }
}

#[cfg(unix)]
fn profile_fallback(profile: &std::path::Path) -> Option<Location> {
    use std::os::unix::ffi::OsStrExt;
    let vault = std::path::absolute(profile.join(VAULT_FALLBACK_NAMES[0])).ok()?;
    Location::local(ProviderPath::from_bytes(vault.as_os_str().as_bytes())).ok()
}

#[cfg(not(unix))]
fn profile_fallback(_profile: &std::path::Path) -> Option<Location> {
    None
}
