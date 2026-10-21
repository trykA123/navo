mod retirement;
mod roots;
mod status;

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use rusty_core::{Location, ProviderPath};
use rusty_fs::{IoQueue, Provider};
use rusty_index::host::{Host, HostOptions, HostStatus};
use rusty_index::pipe::{self, Reply, VolumeStatus};
use rusty_index::search;
use rusty_index::walker::Limits;
use rusty_store::StoreDirs;
use rusty_ui::command_center::index_source::{IndexMetadata, PipeQueries};
use rusty_ui::command_center::{Cancel, IndexHooks, IndexMode, IndexOptions};

use retirement::Retirements;
use roots::{exclusions, is_allowed, roots};

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
    retirement: Retirements,
}

pub fn hooks(dirs: Option<StoreDirs>, provider: Arc<dyn Provider>) -> IndexHooks {
    let retirement = match Retirements::new() {
        Ok(retirement) => retirement,
        Err(error) => {
            tracing::warn!(%error, "cannot start walk index retirement worker");
            return IndexHooks::none();
        }
    };
    let metadata = provider.clone();
    let hosts = Arc::new(Mutex::new(Hosts {
        provider,
        dirs,
        options: None,
        queue: Arc::new(IoQueue::new()),
        roots: Vec::new(),
        desired: Vec::new(),
        retirement,
    }));
    let start = hosts.clone();
    IndexHooks {
        start: Arc::new(move |options| {
            let service_ready = matches!(options.mode, IndexMode::Auto | IndexMode::Service)
                && pipe::Client::connect().is_ok();
            let mut hosts = start.lock().unwrap_or_else(PoisonError::into_inner);
            if service_ready {
                hosts.retire();
                hosts.options = None;
            } else {
                hosts.configure(options, None);
            }
        }),
        status: status::hook(hosts.clone()),
        pipe: PipeQueries::new(Arc::new(query_pipe)),
        host: Arc::new(move |options, query, cancel, service| {
            let mut hosts = hosts.lock().unwrap_or_else(PoisonError::into_inner);
            hosts.configure(options, service);
            hosts.query(query, cancel)
        }),
        metadata: IndexMetadata::new(Arc::new(move |location| metadata.metadata(location))),
    }
}

fn query_pipe(
    options: &IndexOptions,
    query: &pipe::Query,
    cancel: &Cancel,
) -> Result<Reply, pipe::PipeError> {
    let mut client = pipe::Client::connect()?;
    if cancel.is_cancelled() {
        return Err(pipe::PipeError::Cancelled);
    }
    client.send_query(query)?;
    let mut reply = client.receive(&|| cancel.is_cancelled())?;
    let available = rusty_shell::list_drives();
    reply.hits.retain(|hit| {
        Location::new(&hit.path).is_ok_and(|path| is_allowed(options, &path, &available))
    });
    Ok(reply)
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
            reply.hits.extend(search_root(root, query, cancel));
        }
        reply.hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        reply.hits.truncate(pipe::MAX_HITS);
        reply
    }

    fn configure(&mut self, options: &IndexOptions, service: Option<&pipe::Status>) {
        let available = rusty_shell::list_drives();
        let desired = if matches!(options.mode, IndexMode::Off | IndexMode::Service) {
            Vec::new()
        } else {
            roots(options, &available, self.walk_roots(), service)
        };
        let unchanged = self.options.as_ref().is_some_and(|old| {
            old.mode == options.mode
                && old.drives == options.drives
                && old.exclude == options.exclude
                && old.network == options.network
                && old.removable == options.removable
        }) && self.desired == desired;
        if !unchanged {
            self.retire();
            self.options = Some(options.clone());
            self.desired = desired;
        }
        let exclude = exclusions(options, &available);
        for location in self.desired.clone() {
            if !self
                .roots
                .iter()
                .any(|root| root.location.same_place(&location))
            {
                self.start_root(location, &exclude);
            }
        }
    }

    fn start_root(&mut self, location: Location, exclude: &[String]) {
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
                exclude: exclude.to_vec(),
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

fn search_root(root: &Root, query: &pipe::Query, cancel: &Cancel) -> Vec<pipe::Hit> {
    let table = root.host.table();
    let Ok(table) = table.try_read() else {
        return Vec::new();
    };
    search::search(&table, &query.request.terms(), pipe::MAX_HITS, &|| {
        cancel.is_cancelled()
    })
    .into_iter()
    .filter_map(|found| {
        table.parent(found.id)?;
        let path = table.path_with(found.id, if cfg!(windows) { '\\' } else { '/' })?;
        let location = Location::with_provider(
            root.location.provider().clone(),
            ProviderPath::from_text(&path),
        )
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
