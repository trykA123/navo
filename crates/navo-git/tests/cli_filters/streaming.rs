use std::fs;
use std::io::{self, Read};
use std::process::Command;
use std::time::Duration;

use navo_git::{CliGit, CliOptions, FilterNotice, GitError, GitPath, GitSource};

use super::fixture::Fixture;

fn fixture() -> Fixture {
    let fixture = Fixture::new().expect("Git is required for streaming tests");
    fixture.init(&["-b", "main"]);
    fixture.write(".gitattributes", b"data.bin filter=slow -text\n");
    fixture.write("data.bin", &vec![b'x'; 8 * 1024 * 1024]);
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("stream");
    fixture
}

fn cli(fixture: &Fixture, body: &str, timeout: Duration) -> CliGit {
    let script = format!(
        "#!/bin/sh\nif [ \"$4\" = --filters ]; then\ncd \"$2\" || exit 1\necho $$ > .git/filter.pid\n{body}\nfi\nexec git \"$@\"\n"
    );
    CliGit::with_options(
        fixture.repo(),
        &CliOptions {
            git: Some(fixture.script("git-stream", &script)),
            timeout,
            ..CliOptions::default()
        },
    )
    .unwrap()
}

fn pid(fixture: &Fixture) -> String {
    fs::read_to_string(fixture.repo().join(".git/filter.pid"))
        .unwrap()
        .trim()
        .to_owned()
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn git_first_bytes_arrive_before_the_filtered_process_ends() {
    let fixture = fixture();
    fixture.write(".git/smudge.sh", b"sleep 0.05\ncat\n");
    fixture.git_ok(&["config", "filter.slow.smudge", "sh .git/smudge.sh"]);
    let cli = cli(&fixture, "", Duration::from_secs(10));
    let mut blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();
    let mut chunk = [0; 4096];
    assert!(blob.reader.read(&mut chunk).unwrap() > 0);
    assert!(alive(&pid(&fixture)), "Git must still be producing stdout");
}

#[test]
fn git_dropping_the_reader_kills_and_reaps_the_child() {
    let fixture = fixture();
    let cli = cli(&fixture, "", Duration::from_secs(10));
    let blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();
    let pid = pid(&fixture);
    assert!(alive(&pid));
    drop(blob.reader);
    assert!(!alive(&pid), "dropping the reader must reap Git");
}

#[test]
fn git_mid_stream_failure_is_an_error_instead_of_eof() {
    let fixture = fixture();
    let cli = cli(
        &fixture,
        "head -c 65536 /dev/zero\necho filter-broke >&2\nexit 7",
        Duration::from_secs(10),
    );
    let mut blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();
    let mut bytes = Vec::new();
    let error = blob.reader.read_to_end(&mut bytes).unwrap_err();
    assert_eq!(bytes.len(), 65536);
    assert!(error.to_string().contains("filter-broke"));
    assert!(
        blob.finish()
            .unwrap_err()
            .to_string()
            .contains("filter-broke")
    );
}

#[test]
fn git_idle_timeout_stops_a_stalled_filter() {
    let fixture = fixture();
    let cli = cli(&fixture, "exec sleep 20", Duration::from_millis(1500));
    assert!(matches!(
        cli.read_blob_worktree_form("HEAD", &GitPath::from("data.bin")),
        Err(GitError::Timeout)
    ));
    assert!(!alive(&pid(&fixture)));
}

#[test]
fn git_idle_timeout_after_a_chunk_is_a_read_error() {
    let fixture = fixture();
    let cli = cli(
        &fixture,
        "head -c 65536 /dev/zero\nexec sleep 20",
        Duration::from_millis(1500),
    );
    let mut blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();
    let error = io::copy(&mut blob.reader, &mut io::sink()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(!alive(&pid(&fixture)));
}

#[test]
fn git_stderr_flood_does_not_block_stdout() {
    let fixture = fixture();
    let cli = cli(
        &fixture,
        "head -c 131072 /dev/zero >&2\nprintf payload\nexit 0",
        Duration::from_secs(60),
    );
    let mut blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();
    let mut bytes = Vec::new();
    blob.reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"payload");
    assert!(blob.reason.is_none());
    let notices = blob.finish().unwrap();
    let [FilterNotice::Filter { detail }] = notices.as_slice() else {
        panic!("stderr must become a finish notice");
    };
    // The first and the last 8 KiB, joined by "\n...\n".
    assert_eq!(detail.len(), 2 * 8 * 1024 + 5);
}

#[test]
fn git_active_chunks_can_outlast_the_idle_timeout() {
    let fixture = fixture();
    let cli = cli(
        &fixture,
        "for chunk in 1 2 3 4 5 6; do head -c 2048 /dev/zero; sleep 0.3; done\nexit 0",
        Duration::from_millis(1500),
    );
    let mut blob = cli
        .read_blob_worktree_form("HEAD", &GitPath::from("data.bin"))
        .unwrap();
    assert_eq!(
        io::copy(&mut blob.reader, &mut io::sink()).unwrap(),
        6 * 2048
    );
    assert!(blob.finish().unwrap().is_empty());
}

#[test]
fn git_a_required_filter_failure_before_output_is_a_command_error() {
    let fixture = fixture();
    fixture.git_ok(&["config", "filter.slow.smudge", "git missing-smudge-command"]);
    fixture.git_ok(&["config", "filter.slow.required", "true"]);
    let cli = CliGit::new(fixture.repo()).unwrap();
    assert!(matches!(
        cli.read_blob_worktree_form("HEAD", &GitPath::from("data.bin")),
        Err(GitError::Command { .. })
    ));
}

#[test]
fn git_a_stalled_smudge_is_killed_with_its_git_parent() {
    let fixture = fixture();
    fixture.write(
        ".git/stalled.sh",
        b"echo $$ > .git/smudge.pid\nexec sleep 20\n",
    );
    fixture.git_ok(&["config", "filter.slow.smudge", "exec sh .git/stalled.sh"]);
    let cli = cli(&fixture, "", Duration::from_millis(300));
    assert!(matches!(
        cli.read_blob_worktree_form("HEAD", &GitPath::from("data.bin")),
        Err(GitError::Timeout)
    ));
    assert!(!alive(&pid(&fixture)));
    let smudge = std::fs::read_to_string(fixture.repo().join(".git/smudge.pid")).unwrap();
    assert!(!alive(smudge.trim()));
}
