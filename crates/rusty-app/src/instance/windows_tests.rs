use super::super::*;
use super::{ReadFile, event};

#[test]
fn a_pipe_creation_failure_keeps_the_mutex_and_allows_startup() {
    let root = tempfile::tempdir().unwrap();
    let hash = super::super::super::naming::profile_hash(root.path()).unwrap();
    let Acquired::First(mut guard) =
        acquire_with(&hash, |_| Err(io::ErrorKind::PermissionDenied.into())).unwrap()
    else {
        panic!("expected the first instance");
    };
    assert!(guard.pipe.is_none());
    guard.serve(|_| Reply::Ok).unwrap();
    assert!(matches!(
        acquire(&hash, root.path()).unwrap(),
        Acquired::Second(_)
    ));
}

#[test]
fn a_closed_windows_connection_is_reported_as_unanswered() {
    let root = tempfile::tempdir().unwrap();
    let hash = super::super::super::naming::profile_hash(root.path()).unwrap();
    let Acquired::First(mut guard) = acquire(&hash, root.path()).unwrap() else {
        panic!("expected the first instance");
    };
    let Acquired::Second(client) = acquire(&hash, root.path()).unwrap() else {
        panic!("expected the second instance");
    };
    let mut pipe = guard.pipe.take().unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        let stop = pipe::event().unwrap();
        assert!(pipe.connect(&stop).unwrap());
        let mut connection = pipe.connection(&stop, deadline);
        protocol::read_line(&mut connection, deadline, &AtomicBool::new(false)).unwrap();
    });
    let request = Request {
        path: None,
        left: None,
        right: None,
        view: None,
        experimental_retention: false,
    };
    let result = client.send(&request, Duration::from_secs(2));
    server.join().unwrap();
    assert!(matches!(result, Err(Error::Timeout)), "{result:?}");
}

#[test]
fn the_pipe_name_includes_the_current_windows_session() {
    let mut session = 0;
    // SAFETY: the process id is current and session points to writable u32 storage.
    let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    assert_ne!(ok, 0);
    assert_eq!(
        pipe_name("profile-hash").unwrap(),
        wide(&format!("\\\\.\\pipe\\Navo-{session}-profile-hash"))
    );
}

#[test]
fn stopping_during_a_pending_read_returns_promptly() {
    let root = tempfile::tempdir().unwrap();
    let hash = super::super::super::naming::profile_hash(root.path()).unwrap();
    let Acquired::First(mut guard) = acquire(&hash, root.path()).unwrap() else {
        panic!("expected the first instance");
    };
    let mut pipe = guard.pipe.take().unwrap();
    let _client = pipe::Pipe::open(&pipe_name(&hash).unwrap()).unwrap();
    let stop = event().unwrap();
    assert!(pipe.connect(&stop).unwrap());
    let mut request = pipe.overlapped().unwrap();
    let pending = &raw mut request;
    let mut buffer = [0_u8];
    // SAFETY: buffer, request and the pipe remain live until wait_pending cancels and drains the read.
    let started = unsafe {
        ReadFile(
            pipe.handle.as_raw_handle(),
            buffer.as_mut_ptr(),
            1,
            std::ptr::null_mut(),
            pending,
        )
    };
    let start_error = io::Error::last_os_error().raw_os_error();
    let began = Instant::now();
    // SAFETY: stop owns a live manual-reset event until the pending read is drained.
    let signalled = unsafe { SetEvent(stop.as_raw_handle()) };
    let result = pipe.wait_pending(pending, &stop, 5000);
    let elapsed = began.elapsed();
    assert_eq!(started, 0);
    assert_eq!(start_error, Some(super::ERROR_IO_PENDING as i32));
    assert_ne!(signalled, 0);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::ConnectionAborted);
    assert!(elapsed < Duration::from_millis(100));
}

fn serving(root: &Path) -> (Guard, Client, Vec<u16>) {
    let hash = super::super::super::naming::profile_hash(root).unwrap();
    let Acquired::First(mut guard) = acquire(&hash, root).unwrap() else {
        panic!("expected the first instance");
    };
    guard.serve(|_| Reply::Ok).unwrap();
    let Acquired::Second(client) = acquire(&hash, root).unwrap() else {
        panic!("expected the second instance");
    };
    (guard, client, pipe_name(&hash).unwrap())
}

fn ping() -> Request {
    Request {
        path: None,
        left: None,
        right: None,
        view: None,
        experimental_retention: false,
    }
}

#[test]
fn a_silent_client_is_dropped_at_the_deadline_so_the_next_client_is_served() {
    let root = tempfile::tempdir().unwrap();
    let (_guard, client, name) = serving(root.path());
    let _silent = pipe::Pipe::open(&name).unwrap();
    let started = Instant::now();
    assert_eq!(
        client.send(&ping(), Duration::from_secs(3)).unwrap(),
        Reply::Ok
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn dropping_the_guard_while_a_client_holds_the_pipe_returns_within_one_second() {
    let root = tempfile::tempdir().unwrap();
    let (guard, _client, name) = serving(root.path());
    let _holder = pipe::Pipe::open(&name).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    drop(guard);
    assert!(started.elapsed() < Duration::from_secs(1));
}
