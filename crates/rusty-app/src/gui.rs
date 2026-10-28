use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_cache::{CacheKeys, DiskListings, DriveIdentity, ListingCache, Warm};
use rusty_core::{AppState, Location};
use rusty_fs::{LocalFs, ProviderOps, SlowFs};
use rusty_shell::VolumeIdentity;
use rusty_store::StoreDirs;
use rusty_ui::browser::Operations;
use rusty_ui::tokens::Mode;
use rusty_ui::window_view::{
    OpenHooks, OpenInbox, OpenRequest, OpenSender, SaveSession, StartOptions, StartWatch, run,
};

use crate::cli::Start;
use crate::jobs::maintenance::MaintenanceTimer;
use crate::search_hooks::index_hooks;
use crate::startup::{
    home_location, initial_state, path_location, place_panes, save, session_folders,
};
use crate::{
    app_menu, clipboard, contrast, drives, editor, files, jobs, keymap, launch, metrics,
    rename_hooks, retained_hooks, settings, shell_menu, terminal,
};

const START_READ: Duration = Duration::from_millis(50);
const EXIT_WRITE: Duration = Duration::from_millis(300);
const EXIT_CLOSE: Duration = Duration::from_secs(1);

pub(crate) struct Launch {
    pub(crate) experimental_retention: bool,
    pub(crate) path: Option<PathBuf>,
    pub(crate) view: Option<PathBuf>,
    pub(crate) panes: [Option<Location>; 2],
    pub(crate) splash: crate::splash::Splash,
    pub(crate) dirs: Option<StoreDirs>,
    pub(crate) cleanup: crate::profile::Cleanup,
    pub(crate) start_watch: Option<StartWatch>,
    pub(crate) instance_requests: OpenInbox,
    pub(crate) running: Option<crate::crash::Running>,
    pub(crate) crash: crate::crash_report::CrashStart,
    pub(crate) store: crate::crash::store::StoreStart,
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

fn session_state(
    dirs: Option<&StoreDirs>,
    home: &Location,
    panes: [Option<Location>; 2],
    splash: &crate::splash::Splash,
) -> AppState {
    let session = dirs.map(StoreDirs::session_file);
    let no_session = Path::new("");
    let mut state = initial_state(session.as_deref().unwrap_or(no_session), home.clone());
    place_panes(&mut state, panes);
    if let Some(saved) = state
        .window(state.focused_window())
        .and_then(|window| window.placement().first())
    {
        splash.place(saved.monitor);
    }
    metrics::mark("session read");
    state
}

fn local_provider() -> Arc<dyn ProviderOps> {
    let slow_fs_ms = std::env::var("NAVO_SLOW_FS_MS").ok();
    let local: Arc<dyn ProviderOps> = Arc::new(LocalFs::new());
    match SlowFs::parse_delay_ms(slow_fs_ms.as_deref()) {
        Some(delay) => Arc::new(SlowFs::new(local, delay)),
        None => local,
    }
}

pub(crate) fn open(launch: Launch) {
    let mut store = launch.store;
    let dirs = launch.dirs;
    let settings_start = settings::start_from(dirs.as_ref(), launch.experimental_retention);
    let (keymap, _keymap_watch) = keymap::hooks(dirs.as_ref());
    let home = home_location(|name| std::env::var(name).ok());
    let state = session_state(dirs.as_ref(), &home, launch.panes, &launch.splash);
    let local = local_provider();
    let services = profile_services(dirs.as_ref(), &home, launch.experimental_retention, &local);
    let keys = CacheKeys::new(Arc::new(drive_identity));
    let (cache, disk) = listing_cache(dirs.as_ref(), &keys, &state);
    crate::diag::register_listings(&launch.crash.diagnostics.caches, &cache, disk.as_ref());
    metrics::mark("listing cache read");
    let remember = crate::search_hooks::Remember::default();
    let (mut settings, _settings_watch) =
        finish_settings(|| settings_start.finish().into_loaded(), &remember);
    settings::wire_diagnostics(&mut settings, &launch.crash, dirs.as_ref());
    let search = search_hooks(services.memory, remember, dirs.as_ref(), &local);
    let crash_report = finish_startup(&mut store, launch.crash, launch.view.is_some());
    run(
        StartOptions {
            state,
            provider: local,
            on_save: session_saver(dirs, disk.clone(), cache.clone(), keys.clone()),
            mode: Mode::Light,
            operations: services.operations,
            launcher: launch::launcher(),
            terminal: terminal::hooks(&settings.values),
            editor: editor::hooks(|name| std::env::var(name).ok()),
            search,
            files: services.files,
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
            drop_platform: Some(crate::drag::service()),
            start_watch: launch.start_watch,
            open: open_hooks(launch.path, launch.instance_requests, launch.splash),
            crash_report,
            rename: Some(services.rename),
            retained: services.retained,
        },
        Rc::new(rusty_shell::display_scale),
        quitter(disk, launch.cleanup, launch.running, services.maintenance),
    );
}

struct ProfileServices {
    memory: Option<files::SharedDb>,
    files: rusty_ui::window_view::FileHooks,
    operations: Operations,
    rename: rusty_ui::browser::RenameHooks,
    retained: Arc<dyn rusty_ui::browser::RetainedHooks>,
    maintenance: MaintenanceTimer,
}

fn profile_services(
    dirs: Option<&StoreDirs>,
    home: &Location,
    experimental: bool,
    local: &Arc<dyn ProviderOps>,
) -> ProfileServices {
    let (memory, files) = open_files(dirs, experimental);
    let (operations, journal) = operations(dirs, home, experimental, local);
    let rename = rename_hooks::profile_hooks(journal, memory.clone(), dirs, experimental);
    metrics::mark("databases opened");
    ProfileServices {
        memory,
        files,
        operations,
        rename,
        retained: retained_hooks::hooks(dirs, experimental, local.clone()),
        maintenance: MaintenanceTimer::start(dirs, experimental, local.clone()),
    }
}

fn search_hooks(
    memory: Option<files::SharedDb>,
    remember: crate::search_hooks::Remember,
    dirs: Option<&StoreDirs>,
    local: &Arc<dyn ProviderOps>,
) -> rusty_ui::window_view::SearchHooks {
    let mut search = crate::search_hooks::hooks(memory, remember);
    search.index = index_hooks(dirs.cloned(), local.clone(), &mut search.service);
    crate::search_hooks::add_upgrade_notice(&mut search.service, dirs);
    search
}

fn finish_startup(
    store: &mut crate::crash::store::StoreStart,
    mut crash: crate::crash_report::CrashStart,
    viewer: bool,
) -> Option<Arc<dyn rusty_ui::window_view::CrashReportService>> {
    store.share();
    crash.recovery = store.notice.clone();
    mark_window_open(viewer);
    crash.service()
}

fn mark_window_open(viewer: bool) {
    tracing::info!(viewer, "window opening");
    metrics::mark("window opening");
}

fn open_files(
    dirs: Option<&StoreDirs>,
    experimental: bool,
) -> (Option<files::SharedDb>, rusty_ui::window_view::FileHooks) {
    let memory = dirs
        .filter(|dirs| dirs.ensure().is_ok())
        .and_then(|dirs| files::open_memory(&dirs.journal_file()));
    let hooks = files::profile_hooks(memory.clone(), dirs, experimental);
    (memory, hooks)
}

fn open_hooks(path: Option<PathBuf>, inbox: OpenInbox, splash: crate::splash::Splash) -> OpenHooks {
    OpenHooks {
        path: path.and_then(|path| path_location(&path)),
        inbox: Some(inbox),
        on_shown: splash.on_shown(),
    }
}

fn quitter(
    disk: Option<DiskListings>,
    cleanup: crate::profile::Cleanup,
    running: Option<crate::crash::Running>,
    maintenance: MaintenanceTimer,
) -> Rc<dyn Fn()> {
    Rc::new(move || {
        maintenance.stop();
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
    remember: &crate::search_hooks::Remember,
) -> (
    rusty_ui::window_view::SettingsHooks,
    Option<rusty_fs::Watch>,
) {
    let loaded = load();
    metrics::mark("settings read");
    rusty_ui::tokens::set_system_palette_reader(contrast::system_palette);
    settings::live(loaded, remember)
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
    experimental: bool,
    local: &Arc<dyn ProviderOps>,
) -> (Operations, Option<jobs::SharedJournal>) {
    let mut protected = vec![home.clone()];
    let Some(dirs) = dirs else {
        let unavailable =
            unavailable_operations(protected, None, "job-profile-unavailable", experimental);
        return (unavailable, None);
    };
    protected.extend(
        [&dirs.config, &dirs.data]
            .into_iter()
            .filter_map(|path| Location::new(path.to_str()?).ok()),
    );
    let db = dirs.journal_file();
    let journal = dirs.ensure().ok().and_then(|()| {
        jobs::open(&db)
            .inspect_err(|error| tracing::error!(%error, "opening the job journal failed"))
            .ok()
    });
    let Some(journal) = journal else {
        let unavailable = unavailable_operations(
            protected,
            Some(dirs),
            "job-journal-unavailable",
            experimental,
        );
        return (unavailable, None);
    };
    let retention_dirs = dirs.clone();
    let logged = Operations {
        retain_deletes: experimental.then(|| {
            Arc::new(move || jobs::retains_deletes(&retention_dirs))
                as Arc<dyn Fn() -> Option<rusty_core::retention::RetentionPolicy> + Send + Sync>
        }),
        write: Some(jobs::write_factory(dirs.clone(), experimental)),
        log: if experimental {
            jobs::retaining_factory(Arc::clone(&journal), dirs.clone(), local.clone())
        } else {
            jobs::profile_factory(Arc::clone(&journal), dirs.clone())
        },
        protected,
    };
    (logged, Some(journal))
}

fn unavailable_operations(
    protected: Vec<Location>,
    dirs: Option<&StoreDirs>,
    message: &'static str,
    experimental: bool,
) -> Operations {
    if !experimental {
        return Operations {
            protected,
            log: jobs::guard_factory(dirs.cloned()),
            write: Some(jobs::guard_write(dirs.cloned())),
            ..Operations::unlogged()
        };
    }
    Operations {
        retain_deletes: None,
        protected,
        log: Arc::new(move |_| Err(rusty_l10n::tr(message))),
        write: Some(Arc::new(|location| {
            Err(rusty_core::FsError {
                location: Some(location.clone()),
                ..rusty_core::FsError::new(rusty_core::FsErrorKind::AccessDenied)
            })
        })),
    }
}

#[cfg(test)]
mod tests;
