mod config;
mod drives;
mod memory;
mod presence;
mod retirement;
mod roots;
mod status;

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Instant;

use rusty_core::{Location, ProviderPath};
use rusty_fs::{IoQueue, Limits as IoLimits, Provider};
use rusty_index::host::{Host, HostOptions, HostStatus};
use rusty_index::pipe::{self, Reply, VolumeStatus};
use rusty_index::search;
use rusty_index::table::NameTable;
use rusty_index::walker::Limits;
use rusty_store::StoreDirs;
use rusty_ui::command_center::index_source::{IndexMetadata, PipeQueries};
use rusty_ui::command_center::{Cancel, IndexHooks, IndexMode, IndexOptions};

use config::{Configured, same_options};
use drives::DriveCache;
use presence::{Selection, ServicePresence};
use retirement::Retirements;
use roots::{Service, exclusions, is_allowed, roots};

struct Root {
    location: Location,
    host: Host,
    status: HostStatus,
}

struct Hosts {
    provider: Arc<dyn Provider>,
    dirs: Option<StoreDirs>,
    options: Option<IndexOptions>,
    queue: Arc<IoQueue>,
    roots: Vec<Root>,
    desired: Vec<Location>,
    exclude: Vec<String>,
    configured: Configured,
    retirement: Retirements,
    presence: ServicePresence,
}

pub fn hooks(
    dirs: Option<StoreDirs>,
    provider: Arc<dyn Provider>,
    service: &mut rusty_ui::window_view::ServiceHooks,
) -> IndexHooks {
    let metadata = provider.clone();
    let hosts = match create_hosts(dirs, provider) {
        Ok(hosts) => hosts,
        Err(error) => {
            tracing::warn!(%error, "cannot start walk index retirement worker");
            return IndexHooks::none();
        }
    };
    let weak = Arc::downgrade(&hosts);
    rusty_diag::components::register(memory::NAME, move || memory::read(&weak));
    let reset = hosts.clone();
    config::reset_after_actions(
        service,
        Arc::new(move || {
            reset
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .configured
                .reset();
        }),
    );
    let start = hosts.clone();
    IndexHooks {
        start: Arc::new(move |options| {
            let service_ready = matches!(options.mode, IndexMode::Auto | IndexMode::Service)
                && service_running(&connect_logged());
            let mut hosts = start.lock().unwrap_or_else(PoisonError::into_inner);
            hosts.configured.reset();
            if options.mode == IndexMode::Auto {
                hosts
                    .presence
                    .refresh_in_background(&Selection::of(options));
            }
            if service_ready {
                hosts.retire();
                hosts.options = None;
            } else {
                hosts.configure(options, None);
            }
        }),
        status: status::hook(hosts.clone()),
        pipe: pipe_queries(),
        host: Arc::new(move |options, query, cancel, service| {
            let mut hosts = hosts.lock().unwrap_or_else(PoisonError::into_inner);
            hosts.configure(options, service);
            hosts.query(query, cancel)
        }),
        metadata: IndexMetadata::new(Arc::new(move |location| metadata.metadata(location))),
    }
}

fn create_hosts(
    dirs: Option<StoreDirs>,
    provider: Arc<dyn Provider>,
) -> std::io::Result<Arc<Mutex<Hosts>>> {
    Ok(Arc::new(Mutex::new(Hosts {
        provider,
        dirs,
        options: None,
        queue: Arc::new(IoQueue::with_limits(IoLimits::BACKGROUND)),
        roots: Vec::new(),
        desired: Vec::new(),
        exclude: Vec::new(),
        configured: Configured::default(),
        retirement: Retirements::new()?,
        presence: ServicePresence::system(),
    })))
}

fn pipe_queries() -> PipeQueries {
    let drives = DriveCache::new(Arc::new(rusty_shell::list_drives));
    drives.refresh_in_background();
    PipeQueries::new(Arc::new(move |options, query, cancel| {
        let mut reply = query_pipe(query, cancel)?;
        let available = drives.drives();
        reply.hits.retain(|hit| {
            Location::new(&hit.path).is_ok_and(|path| is_allowed(options, &path, &available))
        });
        Ok(reply)
    }))
}

fn query_pipe(query: &pipe::Query, cancel: &Cancel) -> Result<Reply, pipe::PipeError> {
    let mut client = pipe::Client::connect()?;
    if cancel.is_cancelled() {
        return Err(pipe::PipeError::Cancelled);
    }
    client.send_query(query).map_err(after_connect)?;
    client
        .receive(&|| cancel.is_cancelled())
        .map_err(after_connect)
}

fn connect_logged() -> Result<pipe::Client, pipe::PipeError> {
    let connected = pipe::Client::connect();
    if let Err(error) = &connected {
        tracing::warn!(%error, ?error, "cannot connect to the index service pipe");
    }
    connected
}

fn service_running<T>(connected: &Result<T, pipe::PipeError>) -> bool {
    !matches!(connected, Err(pipe::PipeError::NotAvailable))
}

fn after_connect(error: pipe::PipeError) -> pipe::PipeError {
    match error {
        pipe::PipeError::NotAvailable => pipe::PipeError::Io(std::io::ErrorKind::BrokenPipe.into()),
        error => error,
    }
}

impl Hosts {
    fn query(&mut self, query: &pipe::Query, cancel: &Cancel) -> Reply {
        let mut reply = Reply::default();
        for root in &mut self.roots {
            if cancel.is_cancelled() {
                return Reply::default();
            }
            if let Some(status) = root.host.try_status() {
                root.status = status;
            }
            let pending = root.status.progress.is_some()
                || root
                    .host
                    .table()
                    .try_read()
                    .map(|table| table.is_empty() && root.status.errors.is_empty())
                    .unwrap_or(true);
            reply.status.volumes.push(VolumeStatus {
                root: root.location.as_str().to_owned(),
                entries: root
                    .status
                    .progress
                    .map_or(root.status.entries, |progress| progress.entries)
                    as u64,
                progress: pending.then_some(0),
                errors: root.status.errors.clone(),
            });
            reply.hits.extend(search_root(
                &root.location,
                &root.host.table(),
                query,
                cancel,
            ));
        }
        reply.hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        reply.hits.truncate(pipe::MAX_HITS);
        reply
    }

    fn configure(&mut self, options: &IndexOptions, service: Option<&pipe::Status>) {
        self.configure_at(options, service, Instant::now());
    }

    fn configure_at(
        &mut self,
        options: &IndexOptions,
        service: Option<&pipe::Status>,
        now: Instant,
    ) {
        let service = self.configured.query_reply(service, now);
        if !self.configured.is_current(options, service.as_ref(), now) {
            self.reconfigure(options, service, now);
        }
        for location in self.desired.clone() {
            if !self
                .roots
                .iter()
                .any(|root| root.location.same_place(&location))
            {
                self.start_root(location);
            }
        }
    }

    fn reconfigure(&mut self, options: &IndexOptions, service: Option<pipe::Status>, now: Instant) {
        let available = rusty_shell::list_drives();
        let volumes = match options.mode {
            IndexMode::Auto => self.presence.volumes(&Selection::of(options)),
            _ => Some(None),
        };
        let desired = match (options.mode, &volumes) {
            (IndexMode::Off | IndexMode::Service, _) | (_, None) => Vec::new(),
            (_, Some(volumes)) => {
                let service = Service {
                    reply: service.as_ref(),
                    volumes: volumes.as_deref(),
                };
                roots(options, &available, self.walk_roots(), service)
            }
        };
        let unchanged = self
            .options
            .as_ref()
            .is_some_and(|old| same_options(old, options))
            && self.desired == desired;
        if !unchanged {
            self.retire();
            self.options = Some(options.clone());
            self.desired = desired;
        }
        self.exclude = exclusions(options, &available);
        if volumes.is_some() {
            self.configured.settle(options, service, now);
        }
    }

    fn start_root(&mut self, location: Location) {
        if self.retirement.pending(&location) {
            return;
        }
        let Some(snapshot) = self.snapshot(&location) else {
            return;
        };
        let started = Host::start(HostOptions {
            provider: self.provider.clone(),
            queue: self.queue.clone(),
            root: location.clone(),
            snapshot,
            limits: Limits {
                max_entries: u32::MAX as usize,
                exclude: self.exclude.clone(),
            },
        });
        match started {
            Ok(host) => self.roots.push(Root {
                location,
                host,
                status: HostStatus {
                    progress: Some(Default::default()),
                    ..HostStatus::default()
                },
            }),
            Err(error) => tracing::warn!(%error, "cannot start walk index"),
        }
    }

    fn walk_roots(&self) -> Vec<String> {
        let Some(path) = self.dirs.as_ref().map(StoreDirs::settings_file) else {
            return Vec::new();
        };
        match std::fs::read_to_string(path) {
            Ok(text) => match rusty_store::parse_settings(&text) {
                Ok((settings, _)) => settings.search.walk_roots,
                Err(error) => {
                    tracing::warn!(%error, "cannot read walk index roots");
                    Vec::new()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                tracing::warn!(%error, "cannot read walk index settings");
                Vec::new()
            }
        }
    }

    fn snapshot(&self, root: &Location) -> Option<PathBuf> {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        root.as_str().hash(&mut hash);
        let name = format!("{:016x}", hash.finish());
        Some(
            self.dirs
                .as_ref()?
                .data
                .join("index")
                .join(format!("walk-{name}.index")),
        )
    }

    fn retire(&mut self) {
        for root in std::mem::take(&mut self.roots) {
            self.retirement.push(root);
        }
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        self.retire();
    }
}

pub(crate) fn search_root(
    root: &Location,
    table: &RwLock<NameTable>,
    query: &pipe::Query,
    cancel: &Cancel,
) -> Vec<pipe::Hit> {
    let Ok(table) = table.try_read() else {
        return Vec::new();
    };
    search_table(root, &table, query, cancel)
}

pub(crate) fn search_table(
    root: &Location,
    table: &NameTable,
    query: &pipe::Query,
    cancel: &Cancel,
) -> Vec<pipe::Hit> {
    search::search(table, &query.request.terms(), pipe::MAX_HITS, &|| {
        cancel.is_cancelled()
    })
    .into_iter()
    .filter_map(|found| {
        table.parent(found.id)?;
        let path = table.path_with(found.id, if cfg!(windows) { '\\' } else { '/' })?;
        let location =
            Location::with_provider(root.provider().clone(), ProviderPath::from_text(&path))
                .ok()?;
        Some(pipe::Hit {
            path: location.as_str().to_owned(),
            flags: table.flags(found.id)?.bits(),
            score: found.score,
            indices: found.indices,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests;
