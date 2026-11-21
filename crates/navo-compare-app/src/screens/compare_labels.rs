//! Labels of a compare tab's sides: the path or repository, the header root and the tab title.
//! The compare tab builds its Folders header and its title from these.

use std::path::Path;

use navo_compare_cli::args::SideSpec;
use sidewise_kit::prelude::*;

use super::compare_tab::CompareRequest;
use super::folders_head::SideRoot;

/// The path of a `Path` side, the repository of a `GitRev` side, or the address of a `GitUrl`
/// side without its `https://`.
pub(super) fn path_of(spec: &SideSpec) -> &Path {
    match spec {
        SideSpec::Path(path) => path,
        SideSpec::GitRev { repo, .. } => repo,
        SideSpec::GitUrl { url, .. } => Path::new(url.strip_prefix("https://").unwrap_or(url)),
    }
}

/// The header's view of one side: its path (absolute for a local side, kept as given when that
/// fails; the bare address for a remote side), plus the revision for a Git side.
pub(super) fn side_root(spec: &SideSpec) -> SideRoot {
    let path = path_of(spec);
    SideRoot {
        path: match spec {
            SideSpec::GitUrl { .. } => path.to_path_buf(),
            _ => std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
        },
        rev: match spec {
            SideSpec::Path(_) => None,
            SideSpec::GitRev { rev, .. } | SideSpec::GitUrl { rev, .. } => Some(rev.clone()),
        },
    }
}

/// `"<left name> ↔ <right name>"` from the last path segment of each side; a Git side is
/// `name@rev`.
pub(super) fn title_of(request: &CompareRequest) -> SharedString {
    let name = |spec: &SideSpec| {
        let path = path_of(spec);
        let base = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        match spec {
            SideSpec::Path(_) => base,
            SideSpec::GitRev { rev, .. } | SideSpec::GitUrl { rev, .. } => format!("{base}@{rev}"),
        }
    };
    format!("{} ↔ {}", name(&request.left), name(&request.right)).into()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use navo_compare_cli::args::SideSpec;

    use super::{CompareRequest, path_of, title_of};

    /// A remote side.
    fn remote() -> SideSpec {
        SideSpec::GitUrl {
            url: "https://github.com/owner/repo".into(),
            rev: "main".into(),
        }
    }

    /// A remote side and a folder title the tab `repo@main ↔ app`.
    #[test]
    fn a_remote_title_reads_name_at_rev() {
        let request = CompareRequest {
            left: remote(),
            right: SideSpec::Path(PathBuf::from("/w/app")),
            choice: Default::default(),
        };
        assert_eq!(title_of(&request).as_ref(), "repo@main ↔ app");
    }

    /// A local side's header root is absolute; a remote side's root is its bare address.
    #[test]
    fn side_roots_are_absolute_only_for_local_sides() {
        let local = super::side_root(&SideSpec::Path(PathBuf::from("rel")));
        assert!(local.path.is_absolute(), "{}", local.path.display());
        let remote = super::side_root(&remote());
        assert_eq!(remote.path, Path::new("github.com/owner/repo"));
        assert_eq!(remote.rev.as_deref(), Some("main"));
    }

    /// The header shows a remote side without its scheme.
    #[test]
    fn a_remote_path_drops_the_scheme() {
        assert_eq!(path_of(&remote()), Path::new("github.com/owner/repo"));
    }
}
