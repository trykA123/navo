use std::io;
use std::sync::Arc;
use std::time::Duration;

use navo_index::pipe::{CancelToken, SERVICE_REVISION, Server, VolumeStatus, build_id};

use super::answer::{SharedVolume, State, answer};
use super::config::{Drive, select_volumes};
use super::lifecycle::guard_worker;
use super::policy::ServiceError;
use super::search::SearchIndex;
use super::service_log::Log;
use super::storage::Storage;
use super::worker::{Worker, record_error};

pub(super) fn run(
    cancel: &CancelToken,
    ready: impl FnOnce() -> io::Result<()>,
) -> Result<(), ServiceError> {
    let storage = Storage::open(false).map_err(ServiceError::Storage)?;
    let log = Log::open(&storage.directory.join("logs"));
    log.line(format_args!(
        "service start pid={} version={} build={} revision={SERVICE_REVISION}",
        std::process::id(),
        env!("CARGO_PKG_VERSION"),
        build_id()
    ));
    let result = serve(&storage, &log, cancel, ready);
    log.line(format_args!(
        "service stop pid={} ok={}",
        std::process::id(),
        result.is_ok()
    ));
    result
}

fn serve(
    storage: &Storage,
    log: &Log,
    cancel: &CancelToken,
    ready: impl FnOnce() -> io::Result<()>,
) -> Result<(), ServiceError> {
    let config = storage.read_config().map_err(ServiceError::Configuration)?;
    let available = super::volumes::discover().map_err(ServiceError::Configuration)?;
    let drives = select_volumes(&config.search, &available).map_err(ServiceError::Configuration)?;
    let volumes = initial_volumes(&drives, &config.search);
    let letters: String = drives.iter().map(|drive| drive.letter).collect();
    log.line(format_args!("volumes {letters}"));
    std::thread::scope(|scope| {
        let _stop = StopOnDrop(cancel);
        std::thread::Builder::new()
            .name("index-heartbeat".into())
            .spawn_scoped(scope, || super::heartbeat::run(log, &volumes, cancel))
            .map_err(ServiceError::Unexpected)?;
        let mut workers = Vec::new();
        for (drive, volume) in drives.iter().zip(&volumes) {
            let shared = Arc::clone(&volume.shared);
            let mut worker =
                Worker::new(*drive, shared, storage, log.clone()).map_err(ServiceError::Storage)?;
            let shared = Arc::clone(&volume.shared);
            workers.push(
                std::thread::Builder::new()
                    .name("ntfs-index".into())
                    .spawn_scoped(scope, move || {
                        guard_worker(
                            cancel,
                            |message| record_error(&shared, message),
                            || worker.run(cancel),
                        )
                    })
                    .map_err(ServiceError::Unexpected)?,
            );
        }
        ready().map_err(ServiceError::Unexpected)?;
        let result = serve_pipe(&config.sid, &volumes, cancel);
        cancel.cancel();
        result
            .and(join_workers(workers))
            .map_err(ServiceError::Unexpected)
    })
}

fn serve_pipe(sid: &str, volumes: &[SharedVolume], cancel: &CancelToken) -> io::Result<()> {
    let server = super::bind_retry::bind_with_retry(
        cancel,
        || Server::bind(sid),
        |message| {
            tracing::error!("{message}");
            for volume in volumes {
                record_error(&volume.shared, message.clone());
            }
        },
        |delay| super::bind_retry::wait_for_retry(cancel, delay),
    );
    let Some(mut server) = server else {
        return Ok(());
    };
    server
        .serve(
            &|request, query_cancel| answer(volumes, request, query_cancel),
            cancel,
        )
        .map_err(io::Error::other)
}

fn initial_volumes(drives: &[Drive], search: &navo_store::Search) -> Vec<SharedVolume> {
    drives
        .iter()
        .map(|drive| {
            SharedVolume::new(State {
                status: VolumeStatus {
                    root: format!("{}:", drive.letter),
                    progress: Some(0),
                    ..Default::default()
                },
                search: SearchIndex::new(search),
                ..Default::default()
            })
        })
        .collect()
}

fn join_workers(workers: Vec<std::thread::ScopedJoinHandle<'_, io::Result<()>>>) -> io::Result<()> {
    let mut result = Ok(());
    for worker in workers {
        while !worker.is_finished() {
            result = result.and(super::scm::stopping());
            std::thread::park_timeout(Duration::from_millis(500));
        }
        result = result.and(
            worker
                .join()
                .map_err(|_| io::Error::other("index worker failed"))
                .and_then(|result| result),
        );
    }
    result
}

struct StopOnDrop<'a>(&'a CancelToken);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
