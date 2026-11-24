//! Filesystem aliases for F5/F6 from Find results: follows links (symlinks, junctions, mount
//! points) and drops verbatim drive and share prefixes, so two names of one place compare equal
//! before containment checks. A link it cannot read refuses the plan.

use std::collections::HashMap;

use navo_core::{EntryKind, FsErrorKind, Location, ProviderId, ProviderPath, WindowsFoldKey};
use navo_fs::ProviderOps;

/// More links than this on one path is a loop.
const MAX_HOPS: u32 = 40;

/// A location's identity in a hash set: case folded where `same_place` folds case.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) enum Place {
    Folded(ProviderId, WindowsFoldKey),
    Exact(Location),
}

pub(super) fn place(location: &Location) -> Place {
    if location.folds_case() {
        Place::Folded(
            location.provider().clone(),
            location.path().windows_fold_key(),
        )
    } else {
        Place::Exact(location.clone())
    }
}

/// True when `location` is local but not a full path: `C:foo`, `C:`, `foo/bar`, or rooted on
/// no drive (`\foo`; on Windows also `/foo`).
pub(super) fn is_partial(location: &Location) -> bool {
    if !location.is_local() {
        return false;
    }
    let separator = |byte: &u8| matches!(byte, b'\\' | b'/');
    match location.as_str().as_bytes() {
        [letter, b':', rest @ ..] if letter.is_ascii_alphabetic() => {
            !rest.first().is_some_and(separator)
        }
        [first, second, ..] if separator(first) && separator(second) => false,
        [b'/', ..] => cfg!(windows),
        _ => true,
    }
}

/// `\\?\C:\x` as `C:\x` and `\\?\UNC\server\share\x` as `\\server\share\x`. Other verbatim
/// paths (`\\?\Volume{…}\`, devices) have no plain form and stay as they are.
fn plain(location: &Location) -> Location {
    let text = location.path().as_str().filter(|_| location.is_local());
    let Some(rest) = text.and_then(|text| text.strip_prefix(r"\\?\")) else {
        return location.clone();
    };
    let share = rest
        .get(..4)
        .filter(|head| head.eq_ignore_ascii_case(r"UNC\"))
        .and_then(|_| rest.get(4..));
    let plain = match (share, rest.as_bytes()) {
        (Some(share), _) => format!(r"\\{share}"),
        (None, [letter, b':', b'\\', ..] | [letter, b':']) if letter.is_ascii_alphabetic() => {
            rest.to_owned()
        }
        (None, _) => return location.clone(),
    };
    Location::local(ProviderPath::from_text(&plain)).unwrap_or_else(|_| location.clone())
}

/// Resolves locations on one provider, remembering each folder it has resolved.
pub(super) struct Aliases<'a> {
    provider: &'a dyn ProviderOps,
    /// Resolved folders by their exact name: case variants may be different folders.
    known: HashMap<Location, Location>,
    hops: u32,
}

/// Why a location could not be resolved: it is not a full path, its links loop, or a link
/// (or whether a name is one) could not be read.
#[derive(Debug)]
pub(super) enum Unresolved {
    Partial(Location),
    Loop(Location),
    Unreadable(Location),
}

impl<'a> Aliases<'a> {
    pub(super) fn new(provider: &'a dyn ProviderOps) -> Self {
        Self {
            provider,
            known: HashMap::new(),
            hops: 0,
        }
    }

    /// `location` with every link on its path followed, its own name included: where a write
    /// into this folder lands.
    pub(super) fn real(&mut self, location: &Location) -> Result<Location, Unresolved> {
        self.hops = 0;
        self.follow(location)
    }

    /// `location` with the links in its folders followed; a link at the end stays the link,
    /// because copying or replacing a link acts on the link itself.
    pub(super) fn real_entry(&mut self, location: &Location) -> Result<Location, Unresolved> {
        self.hops = 0;
        let plain = plain(location);
        match plain.parent_and_name() {
            (Some(parent), Some(name)) => self
                .follow(&parent)?
                .join(&name)
                .map_err(|_| Unresolved::Partial(location.clone())),
            _ => Ok(plain),
        }
    }

    fn follow(&mut self, location: &Location) -> Result<Location, Unresolved> {
        let plain = plain(location);
        if let Some(found) = self.known.get(&plain) {
            return Ok(found.clone());
        }
        let (here, strict) = match plain.parent_and_name() {
            (Some(parent), Some(name)) => (
                self.follow(&parent)?
                    .join(&name)
                    .map_err(|_| Unresolved::Partial(location.clone()))?,
                true,
            ),
            // A drive or share root may not read as an entry; a top-level link still counts.
            _ => (plain.clone(), false),
        };
        let real = match self.link_at(&here, strict)? {
            Some(to) => {
                self.hops += 1;
                if self.hops > MAX_HOPS {
                    return Err(Unresolved::Loop(location.clone()));
                }
                let to = beside(&here, &to)?;
                self.follow(&to)?
            }
            None => here,
        };
        self.known.insert(plain, real.clone());
        Ok(real)
    }

    /// Where the link at `here` points; `None` when `here` is no link or does not exist yet (a
    /// new folder at the target). A link whose target cannot be read refuses the plan, and so
    /// does an unreadable name when `strict`.
    fn link_at(&self, here: &Location, strict: bool) -> Result<Option<Location>, Unresolved> {
        let unreadable = || Unresolved::Unreadable(here.clone());
        match self.provider.metadata(here) {
            Ok(entry) if entry.kind == EntryKind::Link => self
                .provider
                .link_target(here)
                .ok()
                .flatten()
                .map(Some)
                .ok_or_else(unreadable),
            Ok(_) => Ok(None),
            Err(error) if error.kind == FsErrorKind::NotFound || !strict => Ok(None),
            Err(_) => Err(unreadable()),
        }
    }
}

/// A link's target `to` as a full location: a relative target counts from the link's folder,
/// a rooted one (`\w`) from the link's drive or share. A drive-relative target (`C:foo`)
/// depends on that drive's current folder, so it is unreadable.
fn beside(link: &Location, to: &Location) -> Result<Location, Unresolved> {
    if !is_partial(to) {
        return Ok(to.clone());
    }
    let partial = || Unresolved::Partial(to.clone());
    let text = to.path().as_str().ok_or_else(partial)?;
    let start = match text.as_bytes() {
        [letter, b':', ..] if letter.is_ascii_alphabetic() => {
            return Err(Unresolved::Unreadable(link.clone()));
        }
        [b'\\' | b'/', ..] => std::iter::successors(Some(link.clone()), Location::parent).last(),
        _ => link.parent(),
    };
    text.split(['/', '\\'])
        .try_fold(start.ok_or_else(partial)?, |folder, part| match part {
            "" | "." => Some(folder),
            ".." => folder.parent(),
            name => folder.join(&ProviderPath::from_text(name)).ok(),
        })
        .ok_or_else(partial)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> Location {
        Location::local(ProviderPath::from_text(text)).expect("a location")
    }

    #[test]
    fn verbatim_drive_and_share_names_match_their_plain_forms() {
        let pairs = [
            (r"\\?\C:\w\a.c", r"c:\W\a.c"),
            (r"\\?\UNC\srv\share\x", r"\\SRV\share\x"),
            (r"\\?\unc\srv\share", r"\\srv\share"),
        ];
        for (verbatim, plain_form) in pairs {
            assert_eq!(place(&plain(&local(verbatim))), place(&local(plain_form)));
        }
    }

    #[test]
    fn drive_relative_and_relative_paths_are_partial() {
        for text in ["C:foo", "C:", "c:foo\\bar", "foo/bar", "foo"] {
            assert!(is_partial(&local(text)), "{text}");
        }
        for text in [
            r"C:\foo",
            "C:/foo",
            r"\\srv\share",
            r"\\?\C:\x",
            r"\\?\Volume{1}\x",
        ] {
            assert!(!is_partial(&local(text)), "{text}");
        }
    }

    #[test]
    fn a_path_rooted_on_no_drive_is_partial() {
        assert!(is_partial(&local(r"\foo")));
        assert_eq!(is_partial(&local("/home/x")), cfg!(windows));
    }

    #[test]
    fn a_rooted_link_target_counts_from_the_links_drive_or_share() {
        let cases = [
            (r"C:\a\b\l", r"\w\x", r"C:\w\x"),
            (r"\\srv\share\a\l", r"\w", r"\\srv\share\w"),
            (r"C:\a\b\l", r"..\w", r"C:\a\w"),
        ];
        for (link, to, lands) in cases {
            let found = beside(&local(link), &local(to)).ok();
            assert_eq!(
                found.map(|it| place(&it)),
                Some(place(&local(lands))),
                "{to}"
            );
        }
    }

    #[test]
    fn a_drive_relative_link_target_is_unreadable() {
        let found = beside(&local(r"C:\a\l"), &local("C:foo"));
        assert!(matches!(found, Err(Unresolved::Unreadable(_))), "{found:?}");
    }

    #[test]
    fn verbatim_volume_and_device_paths_keep_their_prefix() {
        for text in [
            r"\\?\Volume{0b1e}\x",
            r"\\?\Volume{0b1e}\",
            r"\\?\GLOBALROOT\Device\x",
        ] {
            assert_eq!(plain(&local(text)), local(text), "{text}");
        }
    }
}
