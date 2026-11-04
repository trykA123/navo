use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use crate::user_commands::expand::{LIST_PATH_MARKER, Plan};
use crate::user_commands::model::Console;

use super::dir::with_list_root;
use super::{
    Liveness, command, create_pid, creation_flags, publish_pid, publish_pid_with, sidecar, start,
    start_with, sweep, sweep_with, write_list,
};

fn plan(
    work: &Path,
    program: impl Into<PathBuf>,
    args: &[&str],
    list: Option<Vec<String>>,
) -> Plan {
    Plan {
        program: program.into(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        start_in: work.to_owned(),
        console: Console::Hide,
        list,
    }
}

/// A program that runs until the marker file exists.
fn held_program(marker: &Path) -> (&'static str, Vec<String>) {
    let marker = marker.to_string_lossy().into_owned();
    if cfg!(windows) {
        let script =
            format!("while (-not (Test-Path '{marker}')) {{ Start-Sleep -Milliseconds 50 }}");
        (
            "powershell",
            vec!["-NoProfile".to_owned(), "-Command".to_owned(), script],
        )
    } else {
        let script = "while [ ! -e \"$0\" ]; do sleep 0.05; done".to_owned();
        ("sh", vec!["-c".to_owned(), script, marker])
    }
}

fn work_dir(root: &tempfile::TempDir) -> PathBuf {
    let parent = root.path().join("work");
    std::fs::create_dir(&parent).expect("a work folder");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("a private work folder");
    }
    let leaf = parent.join("lists");
    std::fs::create_dir(&leaf).expect("a lists folder");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o700))
            .expect("a private lists folder");
    }
    leaf
}

fn list_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("list-") && name.ends_with(".txt"))
        })
        .collect()
}

fn pid_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".pid"))
        })
        .collect()
}

fn age(path: &Path, now: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("a list file")
        .set_modified(now - Duration::from_secs(25 * 60 * 60))
        .expect("an old list file");
}

fn pid_file(path: &Path, pid: &str) {
    std::fs::write(sidecar(path), pid).expect("a pid sidecar");
}

#[test]
fn the_list_file_has_utf8_crlf_lines_and_no_bom() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let list = write_list(&work, &["C:\\a b.txt".to_owned(), "C:\\ü.txt".to_owned()])
        .expect("a list file");
    let bytes = std::fs::read(&list).expect("the list bytes");
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(
        String::from_utf8(bytes).expect("UTF-8"),
        "C:\\a b.txt\r\nC:\\ü.txt\r\n"
    );
    assert!(
        list.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("list-") && name.ends_with(".txt"))
    );
}

#[test]
fn two_lists_at_once_get_two_files() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let (first, second) = std::thread::scope(|scope| {
        let one = scope.spawn(|| write_list(&work, &["C:\\one.txt".to_owned()]));
        let two = scope.spawn(|| write_list(&work, &["C:\\two.txt".to_owned()]));
        (
            one.join().expect("one list").expect("one file"),
            two.join().expect("two lists").expect("two file"),
        )
    });
    assert_ne!(first, second);
    assert!(first.is_file() && second.is_file());
}

#[test]
fn start_writes_the_list_and_removes_it_after_exit() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let marker = root.path().join("release");
    let (program, args) = held_program(&marker);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let plan = plan(
        &work,
        program,
        &args,
        Some(vec!["C:\\a b.txt".to_owned(), "C:\\ü.txt".to_owned()]),
    );
    let (sender, receiver) = mpsc::channel();
    with_list_root(&work, || {
        start(
            plan,
            Box::new(move |code| {
                let _ = sender.send(code);
            }),
        )
    })
    .expect("the command starts");
    let lists = list_files(&work);
    assert_eq!(lists.len(), 1, "{lists:?}");
    let list = &lists[0];
    assert_eq!(
        std::fs::read_to_string(list).expect("the list"),
        "C:\\a b.txt\r\nC:\\ü.txt\r\n"
    );
    let pid = sidecar(list);
    assert!(pid.is_file(), "the pid sidecar");
    std::fs::write(&marker, b"").expect("the release marker");
    assert_eq!(
        receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("the exit"),
        Some(0)
    );
    assert!(!list.exists(), "the list is removed after the exit");
    assert!(!pid.exists(), "the sidecar is removed after the exit");
}

#[test]
fn a_waiter_that_cannot_start_stops_the_start_and_removes_the_list() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let marker = root.path().join("release");
    let (program, args) = held_program(&marker);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let plan = plan(&work, program, &args, Some(vec!["C:\\a.txt".to_owned()]));
    let result = with_list_root(&work, || {
        start_with(plan, Box::new(|_code| {}), |_job| {
            Err(std::io::Error::other("no thread"))
        })
    });
    assert!(result.is_err());
    assert!(list_files(&work).is_empty(), "the list is removed");
    assert!(pid_files(&work).is_empty(), "no sidecar stays");
    std::fs::write(&marker, b"").expect("the release marker");
}

#[test]
fn a_failed_spawn_removes_the_list() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let plan = plan(
        &work,
        work.join("no-such-program"),
        &[],
        Some(vec!["C:\\a.txt".to_owned()]),
    );
    assert!(with_list_root(&work, || start(plan, Box::new(|_code| {}))).is_err());
    assert!(list_files(&work).is_empty(), "the list is removed");
    assert!(pid_files(&work).is_empty(), "no sidecar appears");
}

#[test]
fn no_marker_reaches_a_spawned_argument() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let list = root.path().join("list-test.txt");
    let plan = Plan {
        program: PathBuf::from("tool"),
        args: vec![
            format!("a{LIST_PATH_MARKER}b"),
            LIST_PATH_MARKER.to_owned(),
            "plain".to_owned(),
        ],
        start_in: root.path().to_owned(),
        console: Console::Show,
        list: Some(vec!["C:\\one.txt".to_owned()]),
    };
    let built = command(&plan, Some(&list)).expect("a command");
    let args: Vec<String> = built
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(!args.iter().any(|arg| arg.contains(LIST_PATH_MARKER)));
    assert_eq!(args[0], format!("a{}b", list.display()));
    assert_eq!(args[1], list.display().to_string());
    assert_eq!(args[2], "plain");
    assert!(command(&plan, None).is_err(), "a marker needs a list file");
}

#[test]
fn creation_flags_show_and_hide() {
    assert_eq!(creation_flags(Console::Show), 0x0000_0010 | 0x0000_0200);
    assert_eq!(creation_flags(Console::Hide), 0x0800_0000 | 0x0000_0200);
}

#[test]
fn a_list_path_that_overflows_the_command_line_is_refused() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let list = PathBuf::from("a".repeat(40_000));
    let plan = Plan {
        program: PathBuf::from("tool"),
        args: vec![format!("{LIST_PATH_MARKER}{}", "x".repeat(20_000))],
        start_in: root.path().to_owned(),
        console: Console::Hide,
        list: Some(Vec::new()),
    };
    assert!(command(&plan, Some(&list)).is_err());
}

#[cfg(windows)]
#[test]
fn the_current_process_is_running() {
    assert_eq!(super::is_running(std::process::id()), Liveness::Running);
}

#[cfg(not(windows))]
#[test]
fn a_pid_counts_as_running_where_the_os_cannot_ask() {
    assert_eq!(super::is_running(u32::MAX), Liveness::Running);
}

#[test]
fn a_list_folder_must_be_real_and_private() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let file = root.path().join("not-a-dir");
    std::fs::write(&file, b"x").expect("a file");
    assert!(write_list(&file, &["C:\\a".to_owned()]).is_err());

    let work = work_dir(&root);
    write_list(&work, &["C:\\a".to_owned()]).expect("a private list folder");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&work)
                .expect("the list folder")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let broad = root.path().join("broad");
        std::fs::create_dir(&broad).expect("a broad folder");
        std::fs::set_permissions(&broad, std::fs::Permissions::from_mode(0o777))
            .expect("broad permissions");
        assert!(write_list(&broad, &["C:\\a".to_owned()]).is_err());
        let target = root.path().join("target");
        std::fs::create_dir(&target).expect("a target folder");
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link).expect("a folder link");
        assert!(write_list(&link, &["C:\\a".to_owned()]).is_err());
        let orphan = root.path().join("orphan");
        std::fs::create_dir(&orphan).expect("an orphan folder");
        std::fs::set_permissions(&orphan, std::fs::Permissions::from_mode(0o700))
            .expect("a private orphan");
        assert!(
            write_list(&orphan, &["C:\\a".to_owned()]).is_err(),
            "a private leaf under a shared parent is refused"
        );
    }
}

#[test]
fn a_path_with_a_line_break_is_refused() {
    let root = tempfile::tempdir().expect("a temporary folder");
    assert!(write_list(root.path(), &["/work/x\r\n/sensitive/victim".to_owned()]).is_err());
    assert!(list_files(root.path()).is_empty());
}

#[test]
fn a_pid_sidecar_is_created_new_and_does_not_follow_links() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    with_list_root(&work, || {
        let pid = work.join("list-x.txt.pid");
        create_pid(&pid).expect("a pending sidecar");
        assert_eq!(
            std::fs::read_to_string(&pid).expect("the pending file"),
            "pending"
        );
        publish_pid(&pid, 7).expect("a published pid");
        assert_eq!(std::fs::read_to_string(&pid).expect("the pid"), "7");

        std::fs::write(&pid, "keep").expect("an existing file");
        assert!(create_pid(&pid).is_err());
        assert_eq!(
            std::fs::read_to_string(&pid).expect("the kept file"),
            "keep"
        );

        #[cfg(unix)]
        {
            let target = work.join("target.txt");
            std::fs::write(&target, "safe").expect("a target file");
            let link = work.join("linked.pid");
            std::os::unix::fs::symlink(&target, &link).expect("a sidecar link");
            assert!(create_pid(&link).is_err());
            assert_eq!(
                std::fs::read_to_string(&target).expect("the target"),
                "safe"
            );
        }
    });
}

#[cfg(unix)]
#[test]
fn publishing_a_pid_replaces_the_sidecar_file() {
    use std::os::unix::fs::MetadataExt;

    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    with_list_root(&work, || {
        let pid = work.join("list-x.txt.pid");
        create_pid(&pid).expect("a pending sidecar");
        let before = std::fs::metadata(&pid).expect("the pending sidecar").ino();
        publish_pid(&pid, 123456).expect("a published pid");
        let after = std::fs::metadata(&pid)
            .expect("the published sidecar")
            .ino();
        assert_ne!(before, after, "the pid is renamed into place");
        assert_eq!(std::fs::read_to_string(&pid).expect("the pid"), "123456");
        assert_eq!(
            std::fs::read_dir(&work).expect("the list folder").count(),
            1,
            "only the sidecar remains"
        );
    });
}

#[test]
fn a_failed_pid_publish_keeps_pending_and_leaves_no_temp_file() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    with_list_root(&work, || {
        let pid = work.join("list-x.txt.pid");
        create_pid(&pid).expect("a pending sidecar");
        let error = publish_pid_with(&pid, 123456, |_, _| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        })
        .expect_err("a failed publish");
        assert!(error.contains("pid"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&pid).expect("the pending sidecar"),
            "pending"
        );
        assert_eq!(
            std::fs::read_dir(&work).expect("the list folder").count(),
            1,
            "the temporary file is removed"
        );
    });
}

#[test]
fn sweep_keeps_fresh_and_running_lists_and_removes_the_rest() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let now = SystemTime::now();

    let fresh = write_list(&work, &["C:\\fresh.txt".to_owned()]).expect("a fresh list");
    pid_file(&fresh, "7");
    let running = write_list(&work, &["C:\\running.txt".to_owned()]).expect("a running list");
    age(&running, now);
    pid_file(&running, "42");
    let gone = write_list(&work, &["C:\\gone.txt".to_owned()]).expect("a gone list");
    age(&gone, now);
    pid_file(&gone, "9");
    let missing = write_list(&work, &["C:\\missing.txt".to_owned()]).expect("a missing list");
    age(&missing, now);
    let broken = write_list(&work, &["C:\\broken.txt".to_owned()]).expect("a broken list");
    age(&broken, now);
    pid_file(&broken, "not a pid");
    let unknown = write_list(&work, &["C:\\unknown.txt".to_owned()]).expect("an unknown list");
    age(&unknown, now);
    pid_file(&unknown, "43");
    let pending = write_list(&work, &["C:\\pending.txt".to_owned()]).expect("a pending list");
    age(&pending, now);
    pid_file(&pending, "pending");
    let other = work.join("keep.txt");
    std::fs::write(&other, "not a list").expect("another file");
    age(&other, now);

    with_list_root(&work, || {
        sweep_with(
            now,
            |pid| match pid {
                42 => Liveness::Running,
                43 => Liveness::Unknown,
                _ => Liveness::Gone,
            },
            |_| Some(now - Duration::from_secs(25 * 60 * 60)),
        )
    });

    assert!(fresh.exists(), "fresh lists stay");
    assert!(running.exists(), "old lists with a running pid stay");
    assert!(!gone.exists(), "old lists with a gone pid go");
    assert!(!sidecar(&gone).exists(), "their sidecars go too");
    assert!(!missing.exists(), "old lists without a sidecar go");
    assert!(broken.exists(), "an unreadable sidecar keeps the list");
    assert!(unknown.exists(), "an unknown process keeps the list");
    assert!(pending.exists(), "a pending sidecar keeps the list");
    assert!(other.exists(), "files that are not lists stay");
    assert_eq!(list_files(&work).len(), 5);
}

#[test]
fn sweep_removes_a_list_whose_pending_sidecar_is_old() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let now = SystemTime::now();
    let old_time = Some(now - Duration::from_secs(25 * 60 * 60));

    let crashed = write_list(&work, &["C:\\crashed.txt".to_owned()]).expect("a list");
    age(&crashed, now);
    pid_file(&crashed, "pending");
    age(&sidecar(&crashed), now);
    let recent = write_list(&work, &["C:\\recent.txt".to_owned()]).expect("a list");
    age(&recent, now);
    pid_file(&recent, "pending");

    with_list_root(&work, || {
        sweep_with(now, |_| Liveness::Running, |_| old_time);
    });
    assert!(!crashed.exists(), "an old pending sidecar frees its list");
    assert!(!sidecar(&crashed).exists(), "and goes with it");
    assert!(recent.exists(), "a pending sidecar changed today keeps it");

    let young = write_list(&work, &["C:\\young.txt".to_owned()]).expect("a list");
    age(&young, now);
    pid_file(&young, "pending");
    age(&sidecar(&young), now);
    with_list_root(&work, || {
        sweep_with(now, |_| Liveness::Running, |_| Some(now));
    });
    assert!(young.exists(), "a sidecar created today keeps the list");
}

#[test]
fn sweep_removes_old_pid_temp_files() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let now = SystemTime::now();
    let old = work.join(".pid-old.tmp");
    std::fs::write(&old, "123").expect("an old temp file");
    age(&old, now);
    let fresh = work.join(".pid-fresh.tmp");
    std::fs::write(&fresh, "123").expect("a fresh temp file");
    let other = work.join("keep.tmp");
    std::fs::write(&other, "123").expect("another file");
    age(&other, now);

    with_list_root(&work, || sweep(now, |_| Liveness::Gone));

    assert!(!old.exists(), "an old pid temp file goes");
    assert!(fresh.exists(), "a fresh pid temp file stays");
    assert!(other.exists(), "files that are not pid temp files stay");
}

#[test]
fn a_list_without_a_sidecar_and_a_new_creation_time_stays() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let now = SystemTime::now();
    let list = write_list(&work, &["C:\\new.txt".to_owned()]).expect("a list");
    age(&list, now);

    with_list_root(&work, || sweep(now, |_| Liveness::Gone));

    assert!(list.exists(), "a list created less than a day ago stays");
}

#[test]
fn a_list_without_a_creation_time_stays() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let now = SystemTime::now();
    let list = write_list(&work, &["C:\\unknown.txt".to_owned()]).expect("a list");
    age(&list, now);

    with_list_root(&work, || {
        sweep_with(now, |_| Liveness::Gone, |_| None);
    });

    assert!(list.exists(), "an unknown creation time keeps the list");
}

#[cfg(unix)]
#[test]
fn a_sidecar_that_is_a_link_keeps_the_list() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let now = SystemTime::now();
    let list = write_list(&work, &["C:\\link.txt".to_owned()]).expect("a list");
    age(&list, now);
    let target = work.join("target.txt");
    std::fs::write(&target, "42").expect("a target");
    symlink(&target, sidecar(&list)).expect("a sidecar link");

    with_list_root(&work, || sweep(now, |_| Liveness::Running));

    assert!(list.exists(), "an uncertain sidecar keeps the list");
    assert!(sidecar(&list).is_symlink());
}

#[cfg(windows)]
#[test]
fn a_hidden_command_reports_its_exit_code() {
    let root = tempfile::tempdir().expect("a temporary folder");
    let work = work_dir(&root);
    let plan = plan(&work, "cmd", &["/c", "exit", "3"], None);
    let (sender, receiver) = mpsc::channel();
    with_list_root(&work, || {
        start(
            plan,
            Box::new(move |code| {
                let _ = sender.send(code);
            }),
        )
    })
    .expect("the command starts");
    assert_eq!(
        receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("the exit"),
        Some(3)
    );
}
