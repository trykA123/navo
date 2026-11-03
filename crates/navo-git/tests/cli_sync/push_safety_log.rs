use navo_git::GitError;

use super::cli_remote_support::{repo, with_origin, wrapped};
use super::cli_sync_support::{calls, commit_file, push};

#[test]
fn the_push_runs_the_command_of_d100g_and_nothing_more() {
    let Some(fixture) = repo() else { return };
    with_origin(&fixture);
    commit_file(&fixture, "one.txt", b"1\n");
    let (cli, log) = wrapped(&fixture, "");
    push(&cli).unwrap();
    let head = fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
    let head = String::from_utf8(head).unwrap();
    let all = calls(&log);
    let line = all.lines().find(|line| line.contains(" push ")).unwrap();
    let expected = format!(
        "-c credential.interactive=never -c gc.auto=0 -c maintenance.auto=false -c remote.origin.mirror=false push --porcelain --no-follow-tags --no-recurse-submodules origin {}:refs/heads/main",
        head.trim()
    );
    assert!(line.ends_with(&expected), "{line}");
    assert_eq!(all.matches(" push ").count(), 1);
    let words: Vec<&str> = line.split(' ').collect();
    assert!(words.iter().all(|word| !word.starts_with('+')), "{line}");
    for word in [
        "--force",
        "--delete",
        "--tags",
        "--set-upstream",
        "--mirror",
        "--all",
    ] {
        assert!(!words.contains(&word), "{line}");
    }
}

fn push_failing_with(stderr: &str) -> GitError {
    let Some(fixture) = repo() else {
        return GitError::Cancelled;
    };
    with_origin(&fixture);
    commit_file(&fixture, "one.txt", b"1\n");
    let extra = format!("case \" $* \" in *\" push \"*) {{ {stderr}; }} >&2; exit 128 ;; esac");
    let (cli, _) = wrapped(&fixture, &extra);
    push(&cli).unwrap_err()
}

#[test]
fn an_http_404_or_409_is_not_a_credential_failure() {
    for code in ["404", "409"] {
        let error = push_failing_with(&format!(
            "echo 'fatal: unable to access x: The requested URL returned error: {code}'"
        ));
        assert!(
            matches!(error, GitError::Command { .. }),
            "{code}: {error:?}"
        );
    }
    for code in ["401", "403"] {
        let error = push_failing_with(&format!(
            "echo 'fatal: unable to access x: The requested URL returned error: {code}'"
        ));
        assert!(
            matches!(error, GitError::AuthFailed | GitError::Cancelled),
            "{code}: {error:?}"
        );
    }
}

#[test]
fn a_credential_failure_after_long_stderr_is_still_seen() {
    let error = push_failing_with(
        "head -c 40000 /dev/zero | tr '\\0' x; echo; echo 'fatal: Authentication failed for x'",
    );
    assert!(
        matches!(error, GitError::AuthFailed | GitError::Cancelled),
        "{error:?}"
    );
}
