use std::fs;
use std::path::PathBuf;

use navo_git::GitError;
use navo_git::status::Reason;
use navo_git::trust::{Guard, UNREADABLE_KEY, untrusted_reason};

use super::trust_support::{GitDir, neutralised, pairs};

const BASE: [&str; 2] = ["core.fsmonitor=false", "core.useBuiltinFSMonitor=false"];

#[test]
fn empty_config_is_not_guarded_and_gives_only_the_fsmonitor_pair() {
    let repo = GitDir::new("");
    let guard = repo.guard();
    assert!(!guard.is_guarded());
    assert!(!guard.is_blocked());
    assert!(guard.keys().is_empty());
    assert_eq!(pairs(&guard), BASE);
}

#[test]
fn missing_config_is_not_guarded() {
    let repo = GitDir::new("");
    fs::remove_file(repo.git_dir.join("config")).unwrap();
    let guard = repo.guard();
    assert!(!guard.is_guarded());
    assert_eq!(pairs(&guard), BASE);
}

#[test]
fn a_filter_gives_the_four_neutralising_pairs() {
    let guard = GitDir::new("[filter \"evil\"]\n\tclean = touch pwned\n").guard();
    assert_eq!(
        pairs(&guard),
        [
            BASE[0],
            BASE[1],
            "filter.evil.clean=",
            "filter.evil.smudge=",
            "filter.evil.process=",
            "filter.evil.required=false",
        ]
    );
    assert!(guard.is_guarded());
    assert!(!guard.is_blocked());
    assert_eq!(guard.keys(), ["filter.evil.clean"]);
}

#[test]
fn every_guard_list_key_guards_the_repository() {
    let cases = [
        ("[core]", "fsmonitor = x", "core.fsmonitor"),
        ("[core]", "hooksPath = x", "core.hookspath"),
        (
            "[core]",
            "useBuiltinFSMonitor = true",
            "core.usebuiltinfsmonitor",
        ),
        ("[core]", "sshCommand = x", "core.sshcommand"),
        ("[core]", "gitProxy = x", "core.gitproxy"),
        ("[core]", "askPass = x", "core.askpass"),
        (
            "[core]",
            "alternateRefsCommand = x",
            "core.alternaterefscommand",
        ),
        ("[credential]", "helper = x", "credential.helper"),
        (
            "[credential \"https://h\"]",
            "helper = x",
            "credential.https://h.helper",
        ),
        ("[remote \"o\"]", "uploadpack = x", "remote.o.uploadpack"),
        ("[remote \"o\"]", "receivepack = x", "remote.o.receivepack"),
        ("[remote \"o\"]", "vcs = x", "remote.o.vcs"),
        ("[diff]", "external = x", "diff.external"),
        ("[diff \"d\"]", "textconv = x", "diff.d.textconv"),
        ("[diff \"d\"]", "command = x", "diff.d.command"),
        ("[gpg]", "program = x", "gpg.program"),
        ("[gpg \"ssh\"]", "program = x", "gpg.ssh.program"),
        (
            "[uploadpack]",
            "packObjectsHook = x",
            "uploadpack.packobjectshook",
        ),
        ("[protocol]", "allow = always", "protocol.allow"),
        ("[protocol \"ext\"]", "allow = always", "protocol.ext.allow"),
        ("[filter \"f\"]", "required = true", "filter.f.required"),
    ];
    for (header, line, key) in cases {
        let guard = GitDir::new(&format!("{header}\n\t{line}\n")).guard();
        assert!(guard.is_guarded(), "{key} did not guard");
        assert!(!guard.is_blocked(), "{key} blocked");
        assert_eq!(guard.keys(), [key]);
    }
}

#[test]
fn harmless_keys_do_not_guard() {
    let config = "[core]\n\teditor = vim\n\tautocrlf = true\n[user]\n\tname = admin\n\
                  [remote \"origin\"]\n\turl = https://example.com/r.git\n\
                  [diff]\n\trenames = true\n[gpg]\n\tformat = ssh\n";
    let guard = GitDir::new(config).guard();
    assert!(!guard.is_guarded());
    assert_eq!(pairs(&guard), BASE);
}

#[test]
fn an_included_file_is_read() {
    let repo = GitDir::new("[include]\n\tpath = extra.inc\n");
    repo.write("repo/.git/extra.inc", b"[core]\n\tsshCommand = x\n");
    assert_eq!(repo.guard().keys(), ["core.sshcommand"]);
}

#[test]
fn include_if_targets_are_read_even_when_the_condition_is_false() {
    let repo = GitDir::new("[includeIf \"gitdir:/nowhere/\"]\n\tpath = cond.inc\n");
    repo.write("repo/.git/cond.inc", b"[filter \"hidden\"]\n\tsmudge = x\n");
    assert!(neutralised(&repo.guard(), "hidden"));
}

#[test]
fn nested_includes_resolve_relative_to_the_including_file() {
    let repo = GitDir::new("[include]\n\tpath = sub/a.inc\n");
    repo.write("repo/.git/sub/a.inc", b"[include]\n\tpath = b.inc\n");
    repo.write("repo/.git/sub/b.inc", b"[gpg]\n\tprogram = x\n");
    assert_eq!(repo.guard().keys(), ["gpg.program"]);
}

#[test]
fn absolute_and_home_includes_are_read() {
    let repo = GitDir::new("");
    let absolute = repo.write("elsewhere/abs.inc", b"[diff]\n\texternal = x\n");
    repo.write("home/repo.inc", b"[core]\n\taskPass = x\n");
    let config = format!(
        "[include]\n\tpath = {}\n\tpath = ~/repo.inc\n",
        absolute.display().to_string().replace('\\', "/")
    );
    fs::write(repo.git_dir.join("config"), config).unwrap();
    assert_eq!(repo.guard().keys(), ["core.askpass", "diff.external"]);
}

#[test]
fn global_and_system_config_is_not_read() {
    let repo = GitDir::new("[user]\n\tname = admin\n");
    let hostile = b"[core]\n\tsshCommand = x\n[filter \"global\"]\n\tclean = x\n";
    repo.write("home/.gitconfig", hostile);
    repo.write("home/.config/git/config", hostile);
    let guard = repo.guard();
    assert!(!guard.is_guarded());
    assert_eq!(pairs(&guard), BASE);
}

#[test]
fn config_worktree_is_read() {
    let repo = GitDir::new("");
    repo.write("repo/.git/config.worktree", b"[core]\n\thooksPath = x\n");
    assert_eq!(repo.guard().keys(), ["core.hookspath"]);
}

#[test]
fn a_linked_worktree_reads_the_common_config_and_its_own_config_worktree() {
    let repo = GitDir::new("[filter \"common\"]\n\tclean = x\n");
    repo.write("repo/.git/worktrees/w/commondir", b"../..\n");
    repo.write(
        "repo/.git/worktrees/w/config.worktree",
        b"[core]\n\tgitProxy = x\n",
    );
    let git_dir = repo.git_dir.join("worktrees").join("w");
    let guard = Guard::read_with_home(&git_dir, Some(&repo.home())).unwrap();
    assert!(neutralised(&guard, "common"));
    assert_eq!(guard.keys(), ["core.gitproxy", "filter.common.clean"]);
}

#[test]
fn a_missing_include_is_ignored() {
    let guard = GitDir::new("[include]\n\tpath = gone.inc\n").guard();
    assert!(!guard.is_guarded());
    assert!(!guard.is_blocked());
}

#[test]
fn a_broken_config_is_guarded_and_blocked() {
    for config in [
        "[core\n\tbare = false\n",
        "[core]\n\t= x\n",
        "[filter \"x\n",
    ] {
        let guard = GitDir::new(config).guard();
        assert!(guard.is_guarded(), "{config:?}");
        assert!(guard.is_blocked(), "{config:?}");
        assert_eq!(guard.keys(), [UNREADABLE_KEY]);
    }
}

#[test]
fn a_broken_included_file_blocks() {
    let repo = GitDir::new("[include]\n\tpath = bad.inc\n");
    repo.write("repo/.git/bad.inc", b"[filter \"x\"\n");
    let guard = repo.guard();
    assert!(guard.is_blocked());
    assert!(guard.keys().contains(&UNREADABLE_KEY));
}

#[test]
fn an_unreadable_include_target_blocks() {
    let repo = GitDir::new("[include]\n\tpath = folder\n");
    fs::create_dir_all(repo.git_dir.join("folder")).unwrap();
    assert!(repo.guard().is_blocked());
}

#[test]
fn an_include_cycle_ends() {
    let repo = GitDir::new("[include]\n\tpath = a.inc\n");
    repo.write("repo/.git/a.inc", b"[include]\n\tpath = b.inc\n");
    repo.write(
        "repo/.git/b.inc",
        b"[include]\n\tpath = a.inc\n[core]\n\taskPass = x\n",
    );
    assert!(repo.guard().keys().contains(&"core.askpass"));
}

#[test]
fn an_include_chain_deeper_than_git_allows_blocks() {
    let repo = GitDir::new("[include]\n\tpath = 0.inc\n");
    for depth in 0..12 {
        let next = format!("[include]\n\tpath = {}.inc\n", depth + 1);
        repo.write(&format!("repo/.git/{depth}.inc"), next.as_bytes());
    }
    assert!(repo.guard().is_blocked());
}

#[test]
fn an_include_navo_cannot_resolve_blocks() {
    for path in ["~someone/x.inc", "%(prefix)/etc/x.inc"] {
        let guard = GitDir::new(&format!("[include]\n\tpath = {path}\n")).guard();
        assert!(guard.is_blocked(), "{path}");
    }
    let repo = GitDir::new("[include]\n\tpath = ~/x.inc\n");
    let guard = Guard::read_with_home(&repo.git_dir, None).unwrap();
    assert!(guard.is_blocked());
}

#[test]
fn an_oversized_config_blocks() {
    let mut config = b"[user]\n".to_vec();
    config.extend(std::iter::repeat_n(b"\tname = admin\n".as_slice(), 100_000).flatten());
    assert!(GitDir::with_bytes(&config).guard().is_blocked());
}

#[test]
fn safe_directory_never_reaches_the_args() {
    let guard = GitDir::new("[safe]\n\tdirectory = *\n").guard();
    assert!(
        !guard
            .args()
            .iter()
            .any(|arg| arg.to_string_lossy().contains("safe"))
    );
}

#[test]
fn reading_writes_nothing() {
    let repo = GitDir::new("[include]\n\tpath = ~/x.inc\n[filter \"f\"]\n\tclean = x\n");
    let config = repo.git_dir.join("config");
    let before = (
        fs::read(&config).unwrap(),
        fs::metadata(&config).unwrap().modified().unwrap(),
    );
    let _guard = repo.guard();
    let after = (
        fs::read(&config).unwrap(),
        fs::metadata(&config).unwrap().modified().unwrap(),
    );
    assert_eq!(before, after);
    assert_eq!(fs::read_dir(repo.home()).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&repo.git_dir).unwrap().count(), 1);
}

#[test]
fn unsafe_directory_refusals_map_to_not_trusted() {
    let refused = [
        GitError::UnsafeDirectory {
            path: PathBuf::from("r"),
        },
        command("fatal: detected dubious ownership in repository at 'r'"),
        command("fatal: unsafe repository ('r' is owned by someone else)"),
    ];
    for error in refused {
        assert_eq!(
            untrusted_reason(&error),
            Some(Reason::NotTrusted),
            "{error}"
        );
    }
    for error in [
        GitError::Timeout,
        GitError::GitNotFound,
        command("fatal: bad object"),
    ] {
        assert_eq!(untrusted_reason(&error), None, "{error}");
    }
}

fn command(stderr: &str) -> GitError {
    GitError::Command {
        args: "status".to_owned(),
        status: Some(128),
        stderr: stderr.to_owned(),
    }
}
