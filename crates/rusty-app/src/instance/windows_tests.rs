use super::*;

#[test]
fn a_pipe_creation_failure_keeps_the_mutex_and_allows_startup() {
    let root = tempfile::tempdir().unwrap();
    let hash = super::super::naming::profile_hash(root.path()).unwrap();
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
    let hash = super::super::naming::profile_hash(root.path()).unwrap();
    let Acquired::First(mut guard) = acquire(&hash, root.path()).unwrap() else {
        panic!("expected the first instance");
    };
    let Acquired::Second(client) = acquire(&hash, root.path()).unwrap() else {
        panic!("expected the second instance");
    };
    let mut pipe = guard.pipe.take().unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !pipe.connect().unwrap() {
            protocol::wait_for_connection(deadline).unwrap();
        }
        protocol::read_line(&mut pipe, deadline, &AtomicBool::new(false)).unwrap();
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
