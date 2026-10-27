use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use crate::pipe::{
    CancelToken, Cancellation, PipeError, Query, Reply, Response, frames, serve_instance,
    session::{StopOnDrop, Transport},
};

struct Connection {
    queries: Mutex<mpsc::Receiver<Vec<u8>>>,
    responses: mpsc::SyncSender<Vec<u8>>,
    closed: mpsc::SyncSender<()>,
}

struct Client {
    queries: mpsc::Sender<Vec<u8>>,
    responses: mpsc::Receiver<Vec<u8>>,
    closed: mpsc::Receiver<()>,
}

struct Pipe {
    connections: Mutex<mpsc::Receiver<Arc<Connection>>>,
    active: Mutex<Option<Arc<Connection>>>,
}

fn fixture() -> (Pipe, mpsc::Sender<Arc<Connection>>) {
    let (connections, incoming) = mpsc::channel();
    (
        Pipe {
            connections: Mutex::new(incoming),
            active: Mutex::default(),
        },
        connections,
    )
}

fn connect(listener: &mpsc::Sender<Arc<Connection>>) -> Client {
    let (queries, incoming) = mpsc::channel();
    let (outgoing, responses) = mpsc::sync_channel(0);
    let (closed, disconnected) = mpsc::sync_channel(1);
    listener
        .send(Arc::new(Connection {
            queries: Mutex::new(incoming),
            responses: outgoing,
            closed,
        }))
        .unwrap();
    Client {
        queries,
        responses,
        closed: disconnected,
    }
}

fn receive<T>(receiver: &mpsc::Receiver<T>, cancel: &Cancellation<'_>) -> Result<T, PipeError> {
    while !cancel.is_cancelled() {
        match receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(value) => return Ok(value),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(PipeError::NotAvailable),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    Err(PipeError::Cancelled)
}

impl Transport for Pipe {
    fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError> {
        let next = receive(&self.connections.lock().unwrap(), cancel)?;
        *self.active.lock().unwrap() = Some(next);
        Ok(())
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        if let Some(connection) = self.active.lock().unwrap().take() {
            let _ = connection.closed.send(());
        }
        Ok(())
    }

    fn read_query(&self, cancel: &Cancellation<'_>) -> Result<Query, PipeError> {
        let connection = self.active.lock().unwrap().as_ref().unwrap().clone();
        let bytes = receive(&connection.queries.lock().unwrap(), cancel)?;
        Ok(frames::decode(&bytes)?)
    }

    fn write_response(&self, response: &Response, deadline: Instant) -> Result<(), PipeError> {
        let connection = self.active.lock().unwrap().as_ref().unwrap().clone();
        let mut bytes = frames::encode(response)?;
        loop {
            match connection.responses.try_send(bytes) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(_)) => return Err(PipeError::NotAvailable),
                Err(mpsc::TrySendError::Full(pending)) => bytes = pending,
            }
            if Instant::now() >= deadline {
                return Err(PipeError::TimedOut);
            }
            thread::yield_now();
        }
    }
}

fn assert_client_failure_is_isolated(bytes: Vec<u8>) {
    let (first_pipe, first_listener) = fixture();
    let (second_pipe, second_listener) = fixture();
    let first = connect(&first_listener);
    let second = connect(&second_listener);
    let cancel = CancelToken::new();
    let stopped = CancelToken::new();
    let handler = |_: &_, _: &_| Reply::default();
    thread::scope(|scope| {
        let _cancel = StopOnDrop(cancel.clone());
        let first_worker = scope.spawn(|| serve_instance(&first_pipe, &handler, &cancel, &stopped));
        let second_worker =
            scope.spawn(|| serve_instance(&second_pipe, &handler, &cancel, &stopped));
        first.queries.send(bytes).unwrap();
        first.closed.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!stopped.is_cancelled());
        second
            .queries
            .send(frames::encode(&super::query(2)).unwrap())
            .unwrap();
        for generation in [2, 3] {
            let client = if generation == 2 {
                &second
            } else {
                &connect(&first_listener)
            };
            if generation == 3 {
                client
                    .queries
                    .send(frames::encode(&super::query(generation)).unwrap())
                    .unwrap();
            }
            for _ in 0..2 {
                let bytes = client
                    .responses
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
                let response = frames::decode::<Response>(&bytes).unwrap();
                assert!(matches!(
                    response,
                    Response::Hits { generation: found, .. }
                        | Response::Status { generation: found, .. } if found == generation
                ));
            }
        }
        cancel.cancel();
        assert!(first_worker.join().unwrap().is_ok());
        assert!(second_worker.join().unwrap().is_ok());
    });
}

#[test]
fn client_that_never_reads_is_disconnected_without_stopping_other_instances() {
    assert_client_failure_is_isolated(frames::encode(&super::query(1)).unwrap());
}

#[test]
fn bad_frame_disconnects_only_its_client_and_accepts_again() {
    assert_client_failure_is_isolated(vec![1, 0, 0, 0, 255]);
}

#[test]
fn totals_v1_round_trip_and_legacy_queries_share_a_session() {
    use crate::pipe::{QueryTerm, Request};
    let (pipe, listener) = fixture();
    let client = connect(&listener);
    let cancel = CancelToken::new();
    let stopped = CancelToken::new();
    let expected = totals_reply();
    let handler = |request: &Request, _: &CancelToken| {
        if request.folder_paths().is_some() {
            Reply {
                totals: Some(expected.clone()),
                ..Reply::default()
            }
        } else {
            Reply::default()
        }
    };
    thread::scope(|scope| {
        let _cancel = StopOnDrop(cancel.clone());
        let worker = scope.spawn(|| serve_instance(&pipe, &handler, &cancel, &stopped));
        send_query(
            &client,
            &Query {
                request: Request {
                    terms: vec![QueryTerm::FolderTotalsV1(vec![
                        "C:\\folder".into(),
                        "D:\\missing".into(),
                    ])],
                },
                generation: 1,
            },
        );
        let response: Response = receive_response(&client);
        assert_eq!(
            response,
            Response::FolderTotalsV1 {
                generation: 1,
                reply: expected.clone()
            }
        );
        send_query(&client, &super::query(2));
        assert_legacy_reply(&client);
        cancel.cancel();
        worker.join().unwrap().unwrap();
    });
}

fn send_query(client: &Client, query: &Query) {
    client.queries.send(frames::encode(query).unwrap()).unwrap();
}

fn receive_response(client: &Client) -> Response {
    frames::decode(
        &client
            .responses
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
    )
    .unwrap()
}

fn totals_reply() -> crate::pipe::FolderTotalsReply {
    use crate::pipe::{FolderTotals, FolderTotalsReply};
    FolderTotalsReply {
        totals: vec![
            Some(FolderTotals {
                bytes: 123,
                files: 2,
                folders: 1,
                complete: false,
            }),
            None,
        ],
        busy: false,
    }
}

fn assert_legacy_reply(client: &Client) {
    for status in [false, true] {
        let response: Response = receive_response(client);
        assert!(match response {
            Response::Hits { generation: 2, .. } => !status,
            Response::Status { generation: 2, .. } => status,
            _ => false,
        });
    }
}
