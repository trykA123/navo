//! Tests of the pointer check: network paths in `.git`, `commondir` and `alternates` files.

use super::*;
use crate::fixture::Fixture;
use crate::repo::Repo;

/// The network path spellings every pointer file is tried with.
const NETWORK: [&str; 4] = [
    r"\\host\share\repo",
    "//host/share/repo",
    r"\\?\UNC\host\share\repo",
    r"\??\UNC\host\share\repo",
];

fn opened(path: &Path) -> Result<(), GitProblem> {
    Repo::open_isolated(path).map(|_| ())
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create folder");
    std::fs::write(path, text).expect("write pointer");
}

#[test]
fn a_dot_git_file_naming_a_network_path_is_unsupported() {
    for target in NETWORK {
        let dir = tempfile::tempdir().expect("temp");
        write(&dir.path().join(".git"), &format!("gitdir: {target}\n"));
        assert_eq!(opened(dir.path()), Err(GitProblem::Unsupported), "{target}");
    }
}

#[test]
fn a_commondir_naming_a_network_path_is_unsupported() {
    for target in NETWORK {
        let fixture = Fixture::new();
        let _ = fixture.commit("main", &[("a.txt", b"a")]);
        write(
            &fixture.path().join(".git/commondir"),
            &format!("{target}\n"),
        );
        assert_eq!(
            opened(fixture.path()),
            Err(GitProblem::Unsupported),
            "{target}"
        );
    }
}

#[test]
fn an_alternates_line_naming_a_network_path_is_unsupported() {
    for target in NETWORK {
        let fixture = Fixture::new();
        let _ = fixture.commit("main", &[("a.txt", b"a")]);
        let alternates = fixture.path().join(".git/objects/info/alternates");
        write(&alternates, &format!("# shared\n{target}\\objects\n"));
        assert_eq!(
            opened(fixture.path()),
            Err(GitProblem::Unsupported),
            "{target}"
        );
    }
}

#[test]
fn a_network_path_behind_a_local_alternate_is_unsupported() {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"a")]);
    let objects = fixture.path().join(".git/objects");
    write(&objects.join("info/alternates"), "../../shared/objects\n");
    let shared = fixture.path().join("shared/objects");
    write(&shared.join("info/alternates"), "//host/share/objects\n");
    assert_eq!(opened(fixture.path()), Err(GitProblem::Unsupported));
}

#[test]
fn a_local_alternate_still_opens() {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"a")]);
    let shared = fixture.path().join("shared/objects");
    std::fs::create_dir_all(shared.join("pack")).expect("shared objects");
    let alternates = fixture.path().join(".git/objects/info/alternates");
    write(&alternates, &format!("{}\n", shared.display()));
    assert_eq!(opened(fixture.path()), Ok(()));
}

#[test]
fn a_quoted_or_oversized_alternates_file_is_unsupported() {
    let quoted = Fixture::new();
    let _ = quoted.commit("main", &[("a.txt", b"a")]);
    let alternates = quoted.path().join(".git/objects/info/alternates");
    write(&alternates, "\"\\\\\\\\host\\\\share\"\n");
    assert_eq!(opened(quoted.path()), Err(GitProblem::Unsupported));
    let large = Fixture::new();
    let _ = large.commit("main", &[("a.txt", b"a")]);
    let alternates = large.path().join(".git/objects/info/alternates");
    write(&alternates, &"#".repeat(64 * 1024 + 1));
    assert_eq!(opened(large.path()), Err(GitProblem::Unsupported));
}

#[test]
fn a_dot_git_file_with_a_local_target_opens() {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"a")]);
    let worktree = tempfile::tempdir().expect("temp");
    let target = fixture.path().join(".git");
    write(
        &worktree.path().join(".git"),
        &format!("gitdir: {}\n", target.display()),
    );
    assert_eq!(check(worktree.path()), Ok(()));
}

/// `core.worktree` spellings that name a network share, as written in a config file.
const WORKTREE_CONFIGS: [&str; 6] = [
    "[core]\n\tworktree = //host/share/w\n",
    "[core]\n\tworktree = \"\\\\\\\\host\\\\share\\\\w\" ; quoted\n",
    "[CORE]\n\tWorkTree=\\\\?\\UNC\\host\\share\n",
    "[core] worktree = \\??\\UNC\\host\\share\n",
    "[user]\n\tname = admin\n[ core ]\n# c\n  worktree = \"//host\"/share\n",
    "\u{feff}[core]\nworktree = //host/share\n",
];

#[test]
fn a_core_worktree_naming_a_network_path_is_unsupported() {
    for name in ["config", "config.worktree"] {
        for text in WORKTREE_CONFIGS {
            let fixture = Fixture::new();
            let _ = fixture.commit("main", &[("a.txt", b"a")]);
            let config = fixture.path().join(".git").join(name);
            let old = std::fs::read_to_string(&config).unwrap_or_default();
            write(&config, &format!("{old}{text}"));
            assert_eq!(
                opened(fixture.path()),
                Err(GitProblem::Unsupported),
                "{name}: {text}"
            );
        }
    }
}

#[test]
fn a_core_worktree_in_the_common_config_is_checked() {
    let dir = tempfile::tempdir().expect("temp");
    let git_dir = dir.path().join(".git");
    write(&git_dir.join("commondir"), "../common\n");
    write(&git_dir.join("gitdir"), "../.git\n");
    write(
        &dir.path().join("common/config"),
        "[core]\n\tworktree = //host/share\n",
    );
    assert_eq!(check(dir.path()), Err(GitProblem::Unsupported));
    write(
        &dir.path().join("common/config"),
        "[core]\n\tworktree = ../w\n",
    );
    assert_eq!(check(dir.path()), Ok(()));
}

#[test]
fn a_gitdir_file_naming_a_network_path_is_unsupported() {
    for target in NETWORK {
        let fixture = Fixture::new();
        let _ = fixture.commit("main", &[("a.txt", b"a")]);
        write(&fixture.path().join(".git/commondir"), ".\n");
        write(&fixture.path().join(".git/gitdir"), &format!("{target}\n"));
        assert_eq!(
            opened(fixture.path()),
            Err(GitProblem::Unsupported),
            "{target}"
        );
    }
}

#[test]
fn a_local_core_worktree_still_opens() {
    let fixture = Fixture::new();
    let _ = fixture.commit("main", &[("a.txt", b"a")]);
    let config = fixture.path().join(".git/config");
    let old = std::fs::read_to_string(&config).expect("config");
    write(
        &config,
        &format!("{old}[core \"x\"]\n\tworktree = //host/a\n"),
    );
    assert_eq!(opened(fixture.path()), Ok(()));
}

#[test]
fn a_continuation_line_hides_no_core_worktree() {
    let dir = tempfile::tempdir().expect("temp");
    let git_dir = dir.path().join(".git");
    write(&git_dir.join("commondir"), "../common\n");
    write(&git_dir.join("gitdir"), "../.git\n");
    let config = dir.path().join("common/config");
    write(
        &config,
        "[core]\n\ta = b\\\n[x]\n\tworktree = //host/share\n",
    );
    assert_eq!(check(dir.path()), Err(GitProblem::Unsupported));
    write(
        &config,
        "[core]\n\ta = b\\\r\n[x]\n\tworktree = //host/share\n",
    );
    assert_eq!(check(dir.path()), Err(GitProblem::Unsupported));
}

#[test]
fn an_escaped_backslash_at_line_end_is_not_a_continuation() {
    let dir = tempfile::tempdir().expect("temp");
    let git_dir = dir.path().join(".git");
    write(&git_dir.join("commondir"), "../common\n");
    write(&git_dir.join("gitdir"), "../.git\n");
    write(
        &dir.path().join("common/config"),
        "[core]\n\ta = b\\\\\n\tworktree = ../w\n",
    );
    assert_eq!(check(dir.path()), Ok(()));
}
