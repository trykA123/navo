use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_cache::{CacheKeys, DiskListings, DriveIdentity, ListingCache, Warm};
use rusty_core::{AppState, Location};
use rusty_fs::{LocalFs, ProviderOps};
use rusty_shell::VolumeIdentity;
use rusty_store::StoreDirs;
use rusty_ui::browser::Operations;
use rusty_ui::tokens::Mode;
use rusty_ui::window_view::{
    OpenHooks, OpenInbox, OpenRequest, OpenSender, SaveSession, StartOptions, StartWatch, run,
};

use crate::cli::Start;
use crate::startup::{
    home_location, initial_state, path_location, place_panes, save, session_folders,
};
use crate::{
    app_menu, clipboard, drives, editor, files, jobs, keymap, launch, metrics, settings, shell_menu,
};

const START_READ: Duration = Duration::from_millis(50);
const EXIT_WRITE: Duration = Duration::from_millis(300);
const EXIT_CLOSE: Duration = Duration::from_secs(1);

pub(crate) struct Launch {
    pub(crate) path: Option<PathBuf>,
    pub(crate) view: Option<PathBuf>,
    pub(crate) panes: [Option<Location>; 2],
    pub(crate) dirs: Option<StoreDirs>,
    pub(crate) cleanup: crate::profile::Cleanup,
    pub(crate) start_watch: Option<StartWatch>,
    pub(crate) instance_requests: OpenInbox,
    pub(crate) running: Option<crate::crash::Running>,
    pub(crate) crash: crate::crash_report::CrashStart,
}

pub(crate) fn on_instance_request(
    sender: &OpenSender,
    request: crate::instance::Request,
) -> crate::instance::Reply {
    tracing::info!("instance request received");
    if sender.send(open_request(&request)) {
        crate::instance::Reply::Ok
    } else {
        tracing::warn!("window no longer accepts instance requests");
        crate::instance::Reply::Error
    }
}

fn open_request(request: &crate::instance::Request) -> OpenRequest {
    let location = |path: &Option<PathBuf>| {
        let location = path.as_deref().and_then(path_location);
        if path.is_some() && location.is_none() {
            tracing::warn!("instance request path is not a usable location");
        }
        location
    };
    OpenRequest {
        path: location(&request.path),
        left: location(&request.left),
        right: location(&request.right),
        view: location(&request.view),
    }
}

pub(crate) fn pane_folders(start: &Start) -> Result<[Option<Location>; 2], String> {
    let folder = |path: &Option<PathBuf>, flag: &str| match path {
        None => Ok(None),
        Some(path) if path.is_dir() => path_location(path)
            .map(Some)
            .ok_or_else(|| format!("{flag} {} is not a usable folder path", path.display())),
        Some(path) => Err(format!(
            "{flag} {} does not exist or is not a folder",
            path.display()
        )),
    };
    Ok([
        folder(&start.left, "--left")?,
        folder(&start.right, "--right")?,
    ])
}

pub(crate) fn open(launch: Launch) {
    let dirs = launch.dirs;
    let settings_start = settings::start_from(dirs.as_ref());
    let (keymap, _keymap_watch) = keymap::hooks(dirs.as_ref());
    let home = home_location(|name| std::env::var(name).ok());
    let session = dirs.as_ref().map(StoreDirs::session_file);
    let no_session = Path::new("");
    let mut state = initial_state(session.as_deref().unwrap_or(no_session), home.clone());
    place_panes(&mut state, launch.panes);
    metrics::mark("session read");
    let memory = open_memory(dirs.as_ref());
    let local = Arc::new(LocalFs::new());
    let (operations, _instance) = operations(dirs.as_ref(), &home, local.as_ref());
    metrics::mark("databases opened");
    let keys = CacheKeys::new(Arc::new(drive_identity));
    let (cache, disk) = listing_cache(dirs.as_ref(), &keys, &state);
    crate::diag::register_listings(&launch.crash.diagnostics.caches, &cache, disk.as_ref());
    metrics::mark("listing cache read");
    let (settings, _settings_watch) = finish_settings(|| settings_start.finish().into_loaded());
    tracing::info!(viewer = launch.view.is_some(), "window opening");
    metrics::mark("window opening");
    run(
        StartOptions {
            state,
            provider: local,
            on_save: session_saver(dirs, disk.clone(), cache.clone(), keys.clone()),
            mode: Mode::Light,
            operations,
            launcher: launch::launcher(),
            editor: editor::hooks(|name| std::env::var(name).ok()),
            search: crate::search_hooks::hooks(memory.clone()),
            files: files::hooks(memory),
            kinds: rusty_ui::window_view::Kinds::builtin(),
            drives: drives::hooks(),
            app_menu: app_menu::hooks(std::env::current_exe().ok().as_deref()),
            settings,
            keymap,
            view: launch.view.and_then(|path| path_location(&path)),
            listing_cache: cache,
            cache_keys: keys,
            shell_menu: shell_menu::provider(),
            file_clipboard: Some(clipboard::service()),
            start_watch: launch.start_watch,
            open: open_hooks(launch.path, launch.instance_requests),
            crash_report: launch.crash.service(),
        },
        Rc::new(rusty_shell::display_scale),
        quitter(disk, launch.cleanup, launch.running),
    );
}

fn open_memory(dirs: Option<&StoreDirs>) -> Option<files::SharedDb> {
    dirs.filter(|dirs| dirs.ensure().is_ok())
        .and_then(|dirs| files::open_memory(&dirs.data.join("ferrite.db")))
}

fn open_hooks(path: Option<PathBuf>, inbox: OpenInbox) -> OpenHooks {
    OpenHooks {
        path: path.and_then(|path| path_location(&path)),
        inbox: Some(inbox),
    }
}

fn quitter(
    disk: Option<DiskListings>,
    cleanup: crate::profile::Cleanup,
    running: Option<crate::crash::Running>,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        if let Some(disk) = &disk {
            let _ = disk.close(EXIT_CLOSE);
        }
        if let Some(running) = &running
            && let Err(error) = running.finish()
        {
            tracing::warn!(%error, "cannot remove the running marker");
        }
        cleanup.on_quit();
    })
}

fn finish_settings(
    load: impl FnOnce() -> Option<settings::Loaded>,
) -> (
    rusty_ui::window_view::SettingsHooks,
    Option<rusty_fs::Watch>,
) {
    let loaded = load();
    metrics::mark("settings read");
    settings::live(loaded)
}

fn session_saver(
    dirs: Option<StoreDirs>,
    disk: Option<DiskListings>,
    cache: ListingCache,
    keys: CacheKeys,
) -> SaveSession {
    Rc::new(move |state: &AppState| {
        if let Some(dirs) = dirs.as_ref()
            && dirs.ensure().is_ok()
            && let Err(error) = save(state, &dirs.session_file())
        {
            tracing::warn!(%error, "saving the session failed");
        }
        if let Some(disk) = &disk {
            disk.save_folders(&cache, &keys, &session_folders(state));
            if !disk.flush(EXIT_WRITE) {
                tracing::warn!("listing cache writes skipped at exit");
            }
        }
    })
}

fn listing_cache(
    dirs: Option<&StoreDirs>,
    keys: &CacheKeys,
    state: &AppState,
) -> (ListingCache, Option<DiskListings>) {
    let cache = ListingCache::default();
    let Some(dirs) = dirs.filter(|dirs| dirs.ensure().is_ok()) else {
        return (cache, None);
    };
    let warm = Warm {
        cache: cache.clone(),
        keys: keys.clone(),
        folders: session_folders(state),
        deadline: Instant::now() + START_READ,
    };
    let disk = DiskListings::open(&dirs.data.join("cache.db"), warm);
    (cache.with_disk(disk.clone()), Some(disk))
}

fn drive_identity(letter: char) -> Option<DriveIdentity> {
    rusty_shell::volume_identity(letter).map(|identity| match identity {
        VolumeIdentity::Serial(serial) => DriveIdentity::Serial(serial),
        VolumeIdentity::Remote(remote) => DriveIdentity::Remote(remote),
    })
}

pub(crate) fn operations(
    dirs: Option<&StoreDirs>,
    home: &Location,
    ops: &dyn ProviderOps,
) -> (Operations, Option<File>) {
    let mut protected = vec![home.clone()];
    let Some(dirs) = dirs else {
        let unlogged = Operations {
            protected,
            ..Operations::unlogged()
        };
        return (unlogged, None);
    };
    protected.extend(
        [&dirs.config, &dirs.data]
            .into_iter()
            .filter_map(|path| Location::new(path.to_str()?).ok()),
    );
    let db = dirs.data.join("ferrite.db");
    let journal = dirs.ensure().ok().and_then(|()| {
        jobs::open(&db)
            .inspect_err(|error| tracing::error!(%error, "opening the job journal failed"))
            .ok()
    });
    let instance = jobs::claim(&dirs.data.join("ferrite.lock"));
    let Some(journal) = journal else {
        let unlogged = Operations {
            protected,
            ..Operations::unlogged()
        };
        return (unlogged, instance.and_then(jobs::Instance::share));
    };
    if instance.as_ref().is_some_and(|instance| instance.sole) {
        match jobs::recover(&journal, ops).and_then(|_| jobs::still_pending(&journal)) {
            Ok(pending) => warn_still_pending(&pending),
            Err(error) => {
                tracing::error!(%error, "recovering unfinished jobs failed");
                if error.is_corruption() {
                    crate::crash::record_failed_check(&db);
                }
            }
        }
    }
    let logged = Operations {
        log: jobs::log_factory(journal),
        protected,
    };
    (logged, instance.and_then(jobs::Instance::share))
}

fn warn_still_pending(pending: &[jobs::StillPending]) {
    for job in pending {
        tracing::warn!(
            job = job.job.0,
            files = job.left.len(),
            "files not yet put back after recovery"
        );
    }
}

#[cfg(test)]
mod tests;
