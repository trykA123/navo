use super::*;
use crate::pipe::{CancelToken, Client, PipeError, Reply, Server};
use std::{
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

fn fixture(label: &str) -> (Server, String) {
    let output = std::process::Command::new("whoami.exe")
        .arg("/user")
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8_lossy(&output.stdout);
    let sid = output
        .split_whitespace()
        .find(|field| field.starts_with("S-1-"))
        .unwrap();
    let name = format!(
        r"\\.\pipe\ferrite-index-test-{}-{label}",
        std::process::id()
    );
    let server = Server {
        pipes: crate::pipe::windows::Pipe::bind_test(&name, sid).unwrap(),
    };
    (server, name)
}

fn connection(label: &str) -> (Server, Client, String) {
    let (mut server, name) = fixture(label);
    server.pipes.truncate(1);
    let client = connect_test(&name);
    (server, client, name)
}

#[test]
fn pipe_round_trip_returns_top_500_paths_flags_and_status_in_order() {
    let (mut server, mut client, _) = connection("round-trip");
    let stop = CancelToken::new();
    let handler = |_: &Request, _: &CancelToken| Reply {
        hits: (0..501)
            .map(|score| Hit {
                path: format!(r"C:\work\file-{score}.rs"),
                flags: 3,
                score,
                indices: vec![0],
            })
            .collect(),
        status: Status {
            volumes: vec![VolumeStatus {
                root: r"C:\".into(),
                entries: 501,
                progress: None,
                errors: Vec::new(),
            }],
        },
    };
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| server.serve(&handler, &stop));
        for generation in [1, 2] {
            let reply = client.query(&query(generation)).unwrap();
            assert_eq!(reply.hits.len(), 500);
            assert_eq!(reply.hits[0].path, r"C:\work\file-500.rs");
            assert_eq!(reply.hits[0].flags, 3);
            assert_eq!(reply.status.volumes[0].entries, 501);
        }
        stop.cancel();
        assert!(serving.join().unwrap().is_ok());
    });
}

#[test]
fn pipe_query_times_out_and_closes_the_connection() {
    let (mut server, mut client, _) = connection("timeout");
    let stop = CancelToken::new();
    let (release, blocked) = mpsc::sync_channel::<()>(1);
    let blocked = Mutex::new(blocked);
    let handler = |_: &Request, _: &CancelToken| {
        let _ = blocked.lock().unwrap().recv_timeout(Duration::from_secs(2));
        Reply::default()
    };
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| server.serve(&handler, &stop));
        let started = Instant::now();
        assert!(matches!(client.query(&query(1)), Err(PipeError::TimedOut)));
        assert!(started.elapsed() >= crate::pipe::QUERY_TIMEOUT);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(
            client.query(&query(2)),
            Err(PipeError::NotAvailable)
        ));
        stop.cancel();
        let _ = release.send(());
        assert!(serving.join().unwrap().is_ok());
    });
}

#[test]
fn pipe_newer_generation_cancels_an_older_search_on_the_same_connection() {
    let (mut server, mut client, _) = connection("cancel");
    let stop = CancelToken::new();
    let (started, running) = mpsc::sync_channel(1);
    let (cancelled, observed) = mpsc::sync_channel(1);
    let handler = |request: &Request, cancel: &CancelToken| {
        if request.terms == query(1).request.terms {
            started.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !cancel.is_cancelled() && Instant::now() < deadline {
                thread::yield_now();
            }
            assert!(cancel.is_cancelled());
            cancelled.send(()).unwrap();
        }
        Reply {
            hits: vec![Hit {
                path: r"C:\new.rs".into(),
                flags: 0,
                score: 1,
                indices: Vec::new(),
            }],
            status: Status::default(),
        }
    };
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| server.serve(&handler, &stop));
        client.send_query(&query(1)).unwrap();
        running.recv_timeout(Duration::from_secs(2)).unwrap();
        let newer = Query {
            generation: 2,
            request: Request::default(),
        };
        client.send_query(&newer).unwrap();
        let reply = client.receive(&|| false).unwrap();
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(reply.hits[0].path, r"C:\new.rs");
        stop.cancel();
        assert!(serving.join().unwrap().is_ok());
    });
}

fn connect_test(name: &str) -> Client {
    Client {
        pipe: Some(crate::pipe::windows::Pipe::connect_test(name).unwrap()),
        pending: None,
    }
}

#[test]
fn client_refuses_a_pipe_owned_by_this_process() {
    let (server, name) = fixture("untrusted-owner");
    assert!(matches!(
        Client::connect_at(&name),
        Err(PipeError::NotAvailable)
    ));
    assert!(matches!(
        server.pipes[0].verify_owner(),
        Err(PipeError::NotAvailable)
    ));
}

#[test]
fn second_client_connects_while_the_first_stays_connected() {
    let (mut server, name) = fixture("multiple");
    let mut first = connect_test(&name);
    let stop = CancelToken::new();
    let handler = |_: &Request, _: &CancelToken| Reply::default();
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| server.serve(&handler, &stop));
        first.query(&query(1)).unwrap();
        let mut second = connect_test(&name);
        second.query(&query(1)).unwrap();
        first.query(&query(2)).unwrap();
        stop.cancel();
        assert!(serving.join().unwrap().is_ok());
    });
}

#[test]
fn early_disconnect_does_not_wedge_accept() {
    let (mut server, early, name) = connection("early-disconnect");
    drop(early);
    let stop = CancelToken::new();
    let handler = |_: &Request, _: &CancelToken| Reply::default();
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| server.serve(&handler, &stop));
        let mut client = connect_test(&name);
        client.query(&query(1)).unwrap();
        stop.cancel();
        assert!(serving.join().unwrap().is_ok());
    });
}

#[test]
fn cancelled_receive_keeps_the_connection_for_a_newer_query() {
    let (mut server, mut client, _) = connection("cancel-receive");
    let stop = CancelToken::new();
    let (started, running) = mpsc::sync_channel(1);
    let handler = |request: &Request, cancel: &CancelToken| {
        if !request.terms.is_empty() {
            started.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !cancel.is_cancelled() && Instant::now() < deadline {
                thread::yield_now();
            }
            assert!(cancel.is_cancelled());
        }
        Reply::default()
    };
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| server.serve(&handler, &stop));
        client.send_query(&query(1)).unwrap();
        running.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            client.receive(&|| true),
            Err(PipeError::Cancelled)
        ));
        assert!(client.pipe.is_some());
        client
            .query(&Query {
                request: Request::default(),
                generation: 2,
            })
            .unwrap();
        stop.cancel();
        assert!(serving.join().unwrap().is_ok());
    });
}

#[test]
fn cancelled_partial_reply_is_preserved_and_filtered_after_a_new_query() {
    let (server, mut client, _) = connection("partial-reply");
    let stop = CancelToken::new();
    let (sent, ready) = mpsc::sync_channel(1);
    thread::scope(|scope| {
        let _stop = crate::pipe::StopOnDrop(stop.clone());
        let serving = scope.spawn(|| send_partial_reply(&server.pipes[0], &stop, sent));
        client.send_query(&query(1)).unwrap();
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        let cancelled_at = Instant::now() + Duration::from_millis(20);
        assert!(matches!(
            client.receive(&|| Instant::now() >= cancelled_at),
            Err(PipeError::Cancelled)
        ));
        client.query(&query(2)).unwrap();
        serving.join().unwrap();
    });
}

fn send_partial_reply(
    pipe: &crate::pipe::windows::Pipe,
    stop: &CancelToken,
    sent: mpsc::SyncSender<()>,
) {
    let tokens = [stop];
    let cancel = crate::pipe::Cancellation::Tokens(&tokens);
    pipe.accept(&cancel).unwrap();
    let _: Query = pipe.read(None, &cancel).unwrap();
    let hits = Response::Hits {
        generation: 1,
        hits: Vec::new(),
    };
    let frame = frames::encode(&hits).unwrap();
    pipe.write_bytes(&frame[..5], None).unwrap();
    sent.send(()).unwrap();
    let next: Query = pipe.read(None, &cancel).unwrap();
    assert_eq!(next.generation, 2);
    pipe.write_bytes(&frame[5..], None).unwrap();
    pipe.write(
        &Response::Status {
            generation: 1,
            status: Status::default(),
        },
        None,
    )
    .unwrap();
    pipe.write(
        &Response::Hits {
            generation: 2,
            hits: Vec::new(),
        },
        None,
    )
    .unwrap();
    pipe.write(
        &Response::Status {
            generation: 2,
            status: Status::default(),
        },
        None,
    )
    .unwrap();
}
