//! The Start screen's Git probe: opens the typed repository and resolves its revision.
//! It blocks, so the Start screen calls `probe` only from the off-thread side check (NC195).

use std::cell::Cell;
use std::path::Path;

use navo_compare_cli::args::{self, ArgError, SideSpec};

use crate::start::SideHint;

/// Classifies one typed side against the disk (NS3, NS17); blocks, so it runs off the UI thread.
/// A well-formed `https://` address gives `Remote` and a refused one `BadAddress`, with no network.
/// A path that does not exist, and a `repo@` with no revision, give `NotFound`.
pub(crate) fn probe(text: &str, cwd: &Path) -> SideHint {
    let found = Cell::new(false);
    let exists = |path: &Path| {
        let answer = path.exists();
        found.set(found.get() || answer);
        answer
    };
    match args::side(text, cwd, &exists) {
        Ok(spec @ SideSpec::GitRev { .. }) => git_hint(spec),
        Ok(spec @ SideSpec::Path(_)) if found.get() => SideHint::Path(spec),
        Ok(spec @ SideSpec::GitUrl { .. }) => SideHint::Remote(spec),
        Err(ArgError::BadUrl(_)) => SideHint::BadAddress,
        Ok(SideSpec::Path(_)) | Err(_) => SideHint::NotFound,
    }
}

/// Resolves a `repo@rev` side: `GitRef` when the revision names one commit, else the problem.
pub(crate) fn git_hint(spec: SideSpec) -> SideHint {
    let SideSpec::GitRev { repo, rev } = &spec else {
        return SideHint::NotFound;
    };
    match sidewise_git::repo::probe(repo, rev) {
        Ok(_) => SideHint::GitRef(spec),
        Err(problem) => SideHint::GitProblem(problem),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::start_tests::TempDir;
    use sidewise_git::fixture::Fixture;
    use sidewise_git::repo::GitProblem;

    fn spec(repo: &std::path::Path, rev: &str) -> SideSpec {
        SideSpec::GitRev {
            repo: repo.to_path_buf(),
            rev: rev.into(),
        }
    }

    #[test]
    fn a_resolving_revision_is_a_git_ref() {
        let fixture = Fixture::new();
        let _ = fixture.commit("main", &[("a.txt", b"one\n")]);
        let side = spec(fixture.dir.path(), "main");
        assert_eq!(git_hint(side.clone()), SideHint::GitRef(side));
    }

    #[test]
    fn an_unknown_revision_is_a_problem() {
        let fixture = Fixture::new();
        let _ = fixture.commit("main", &[("a.txt", b"one\n")]);
        let hint = git_hint(spec(fixture.dir.path(), "nope"));
        assert_eq!(hint, SideHint::GitProblem(GitProblem::NoSuchRevision));
    }

    #[test]
    fn a_plain_folder_is_not_a_repository() {
        let dir = TempDir::new("plainfolder");
        let hint = git_hint(spec(dir.path(), "main"));
        assert_eq!(hint, SideHint::GitProblem(GitProblem::NotRepository));
    }
}
