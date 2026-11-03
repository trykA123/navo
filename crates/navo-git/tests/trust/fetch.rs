use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use navo_git::trust::Guard;

#[cfg(unix)]
use navo_git::trust::NO_AUTO_MAINTENANCE;

use super::fixture::Fixture;
use super::trust_support::git_fixture;

const MARKER: &str = "hook-ran";
const CONFIGURED_HOOKS: (u32, u32) = (2, 54);

struct Downstream {
    fixture: Fixture,
    down: PathBuf,
}

impl Downstream {
    fn new(config: &str) -> Option<Self> {
        let fixture = git_fixture()?;
        fixture.init(&["-b", "main"]);
        fixture.write("a.txt", b"a\n");
        fixture.git_ok(&["add", "."]);
        fixture.commit("base");
        let down = fixture.repo().with_file_name("down");
        fixture.git_ok(&["clone", "-q", ".", down.to_str().unwrap()]);
        let clone = Self { fixture, down };
        let path = clone.down.join(".git").join("config");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str(config);
        fs::write(path, text).unwrap();
        Some(clone)
    }

    #[cfg(unix)]
    fn git_ok(&self, args: &[&str]) -> Vec<u8> {
        let mut all = vec!["-C", self.down.to_str().unwrap()];
        all.extend_from_slice(args);
        self.fixture.git_ok(&all).stdout
    }

    #[cfg(unix)]
    fn two_packs_and_loose_garbage(&self) {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, SystemTime};

        for name in ["b.txt", "c.txt"] {
            fs::write(self.down.join(name), name).unwrap();
            self.git_ok(&["add", name]);
            self.git_ok(&["commit", "-q", "-m", name]);
            self.git_ok(&["repack", "-q"]);
        }
        let blob = self.down.join("garbage");
        fs::write(&blob, b"unreachable\n").unwrap();
        let oid = self.git_ok(&["hash-object", "-w", blob.to_str().unwrap()]);
        fs::remove_file(blob).unwrap();
        let oid = String::from_utf8(oid).unwrap();
        let (dir, file) = oid.trim().split_at(2);
        let loose = self.down.join(".git/objects").join(dir).join(file);
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        fs::set_permissions(&loose, fs::Permissions::from_mode(0o644)).unwrap();
        fs::File::options()
            .write(true)
            .open(loose)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }

    fn fetch_ran_hook(&self, args: &[OsString], flags: &[&str]) -> bool {
        let mut next = fs::read(self.fixture.repo().join("a.txt")).unwrap();
        next.push(b'a');
        self.fixture.write("a.txt", &next);
        self.fixture.git_ok(&["commit", "-q", "-a", "-m", "next"]);
        let mut all = vec!["-C", self.down.to_str().unwrap()];
        all.extend(args.iter().map(|arg| arg.to_str().unwrap()));
        all.push("fetch");
        all.extend_from_slice(flags);
        all.extend(["-q", "origin"]);
        self.fixture.git_ok(&all);
        let head = self.fixture.git_ok(&["rev-parse", "HEAD"]).stdout;
        let fetched = self.fixture.git_ok(&[
            "-C",
            self.down.to_str().unwrap(),
            "rev-parse",
            "origin/main",
        ]);
        assert_eq!(fetched.stdout, head, "the fetch did not land");
        let marker = self.down.join(MARKER);
        let ran = marker.exists();
        fs::remove_file(marker).unwrap_or_default();
        ran
    }

    fn guard(&self) -> Guard {
        Guard::read(&self.down.join(".git")).unwrap()
    }

    #[cfg(unix)]
    fn worktree(&self) -> &std::path::Path {
        &self.down
    }
}

#[test]
fn a_configured_hook_guards_the_repository() {
    let config = "[hook \"x\"]\n\tevent = reference-transaction\n\tcommand = touch hook-ran\n";
    let Some(clone) = Downstream::new(config) else {
        return;
    };
    let guard = clone.guard();
    assert_eq!(guard.keys(), ["hook.x.command", "hook.x.event"]);
    assert!(guard.is_guarded() && !guard.is_blocked());
    let ran = clone.fetch_ran_hook(&[], &[]);
    if clone.fixture.version() >= CONFIGURED_HOOKS {
        assert!(ran, "the fixture never runs it");
    }
}

#[cfg(unix)]
#[test]
fn background_fetch_never_runs_the_recent_objects_hook() {
    let config = "[gc]\n\trecentObjectsHook = touch hook-ran\n\tauto = 1\n\
                  \tautoPackLimit = 1\n\tautoDetach = false\n[maintenance]\n\tstrategy = gc\n";
    for guarded in [false, true] {
        let Some(clone) = Downstream::new(config) else {
            return;
        };
        clone.two_packs_and_loose_garbage();
        let guard = clone.guard();
        assert_eq!(guard.keys(), ["gc.recentobjectshook"]);
        let ran = if guarded {
            clone.fetch_ran_hook(&guard.fetch_args(), &[NO_AUTO_MAINTENANCE])
        } else {
            clone.fetch_ran_hook(&[], &[])
        };
        assert_eq!(ran, !guarded, "guarded: {guarded}");
    }
}

#[cfg(unix)]
#[test]
fn background_fetch_never_runs_a_hook_file() {
    use std::os::unix::fs::PermissionsExt;

    let Some(clone) = Downstream::new("") else {
        return;
    };
    let hook = clone.worktree().join(".git/hooks/reference-transaction");
    fs::write(&hook, "#!/bin/sh\ntouch hook-ran\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let guard = clone.guard();
    assert!(!guard.is_guarded());
    assert!(!clone.fetch_ran_hook(&guard.fetch_args(), &[NO_AUTO_MAINTENANCE]));
    assert!(clone.fetch_ran_hook(&[], &[]), "the fixture never runs it");
}
