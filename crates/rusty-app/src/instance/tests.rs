use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{Acquired, Client, Guard, Reply, Request, acquire_in};

fn first(profile: &Path, runtime: &Path) -> Guard {
    match acquire_in(profile, runtime).unwrap() {
        Acquired::First(guard) => guard,
        Acquired::Second(_) => panic!("expected the first instance"),
    }
}

fn second(profile: &Path, runtime: &Path) -> Client {
    match acquire_in(profile, runtime).unwrap() {
        Acquired::Second(client) => client,
        Acquired::First(_) => panic!("expected the second instance"),
    }
}

fn request() -> Request {
    let cwd = std::env::current_dir().unwrap();
    Request {
        path: Some(cwd.join("work")),
        left: Some(cwd.join("left")),
        right: Some(cwd.join("right")),
        view: Some(cwd.join("note.txt")),
    }
}

#[test]
fn requests_serialize_as_one_versioned_json_line_with_absolute_paths() {
    let request = request();
    let line = request.to_line().unwrap();
    assert_eq!(line.lines().count(), 1);
    assert!(line.ends_with('\n'));
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["v"], 1);
    assert_eq!(
        value["path"],
        request.path.as_ref().unwrap().to_str().unwrap()
    );
    assert_eq!(Request::parse(&line).unwrap(), request);
    assert!(Request::parse("{\"v\":2}").is_err());
    assert!(Request::parse("{\"v\":1,\"left\":\"relative\"}").is_err());
}

#[test]
fn request_paths_resolve_against_the_sending_working_folder() {
    let root = tempfile::tempdir().unwrap();
    let start = crate::cli::Start {
        path: Some("missing".into()),
        left: Some("left".into()),
        right: Some(root.path().join("right")),
        view: Some("note.txt".into()),
        ..crate::cli::Start::default()
    };
    assert_eq!(
        Request::from_start(&start, root.path()).unwrap(),
        Request {
            path: Some(root.path().join("missing")),
            left: Some(root.path().join("left")),
            right: Some(root.path().join("right")),
            view: Some(root.path().join("note.txt")),
        }
    );
}

#[test]
fn first_acquire_holds_the_lock_and_second_is_a_client() {
    let root = tempfile::tempdir().unwrap();
    let _guard = first(root.path(), root.path());
    let _client = second(root.path(), root.path());
    let child = root.path().join("child");
    std::fs::create_dir(&child).unwrap();
    let _same_folder = second(&child.join(".."), root.path());
    let _other_profile = first(&root.path().join("other-profile"), root.path());
}

#[test]
fn a_request_round_trips_to_the_handler() {
    let root = tempfile::tempdir().unwrap();
    let mut guard = first(root.path(), root.path());
    let (sent, received) = mpsc::channel();
    guard
        .serve(move |request| {
            sent.send(request).unwrap();
            Reply::Ok
        })
        .unwrap();
    let client = second(root.path(), root.path());
    assert_eq!(
        client.send(&request(), Duration::from_secs(2)).unwrap(),
        Reply::Ok
    );
    assert_eq!(
        received.recv_timeout(Duration::from_secs(1)).unwrap(),
        request()
    );
}

#[cfg(unix)]
fn socket(runtime: &Path) -> std::path::PathBuf {
    std::fs::read_dir(runtime)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "sock"))
        .unwrap()
}

#[cfg(unix)]
#[test]
fn malformed_lines_and_unknown_versions_get_error_without_a_handler_call() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    let root = tempfile::tempdir().unwrap();
    let mut guard = first(root.path(), root.path());
    guard
        .serve(|_| panic!("invalid requests must not reach the handler"))
        .unwrap();
    for line in [
        "not json\n",
        "{\"v\":2}\n",
        "{\"v\":1,\"path\":\"relative\"}\n",
    ] {
        let mut stream = UnixStream::connect(socket(root.path())).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(line.as_bytes()).unwrap();
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).unwrap();
        assert_eq!(reply, "error\n");
    }
}

#[cfg(unix)]
#[test]
fn a_stale_socket_without_a_lock_holder_is_recovered() {
    use std::os::unix::net::UnixListener;
    let root = tempfile::tempdir().unwrap();
    let guard = first(root.path(), root.path());
    let address = socket(root.path());
    drop(guard);
    let stale = UnixListener::bind(&address).unwrap();
    drop(stale);
    let mut guard = first(root.path(), root.path());
    guard.serve(|_| Reply::Ok).unwrap();
    assert_eq!(
        second(root.path(), root.path())
            .send(&request(), Duration::from_secs(2))
            .unwrap(),
        Reply::Ok
    );
}

#[test]
fn the_client_times_out_when_the_server_never_replies() {
    let root = tempfile::tempdir().unwrap();
    let mut guard = first(root.path(), root.path());
    let (release, wait) = mpsc::channel();
    let wait = std::sync::Mutex::new(wait);
    guard
        .serve(move |_| {
            wait.lock().unwrap().recv().unwrap();
            Reply::Ok
        })
        .unwrap();
    let client = second(root.path(), root.path());
    let started = Instant::now();
    let result = client.send(&request(), Duration::from_millis(100));
    let elapsed = started.elapsed();
    release.send(()).unwrap();
    assert!(result.is_err());
    assert!(elapsed >= Duration::from_millis(90), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
}

#[test]
fn dropping_the_guard_releases_the_lock() {
    let root = tempfile::tempdir().unwrap();
    let mut guard = first(root.path(), root.path());
    guard.serve(|_| Reply::Ok).unwrap();
    drop(guard);
    let _replacement = first(root.path(), root.path());
}

#[cfg(windows)]
#[test]
fn a_unique_windows_mutex_and_pipe_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("unique-windows-profile");
    let mut guard = first(&profile, root.path());
    guard.serve(|_| Reply::Ok).unwrap();
    let client = second(&profile, root.path());
    assert_eq!(
        client.send(&request(), Duration::from_secs(2)).unwrap(),
        Reply::Ok
    );
}
