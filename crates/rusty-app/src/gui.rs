mod profile;
mod quit;

#[cfg(test)]
use profile::operations;
use profile::profile_services;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_cache::{CacheKeys, DiskListings, DriveIdentity, ListingCache, Warm};
use rusty_core::{AppState, Location};
use rusty_fs::{LocalFs, ProviderOps, SlowFs};
use rusty_shell::VolumeIdentity;
use rusty_store::StoreDirs;
use rusty_ui::tokens::Mode;
use rusty_ui::window_view::{
    BackgroundSaveSession, OpenHooks, OpenInbox, OpenRequest, OpenSender, RunHooks, SaveSession,
    StartOptions, StartWatch, run,
};

use self::quit::quitter;
use crate::cli::Start;
use crate::search_hooks::{Visits, index_hooks};
use crate::startup::{
    home_location, initial_state, path_location, place_panes, save, session_folders,
};
use crate::{
    app_menu, clipboard, contrast, drives, editor, files, keymap, launch, metrics, settings,
    shell_menu, terminal,
};

const START_READ: Duration = Duration::from_millis(50);
const EXIT_WRITE: Duration = Duration::from_millis(300);
const PDF_WARM_DELAY: Duration = Duration::from_secs(3);

pub(crate) struct Launch {
    pub(crate) trace: Option<rusty_ui::window_view::InputTrace>,
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

fn local_provider(trace: Option<&rusty_ui::window_view::InputTrace>) -> Arc<dyn ProviderOps> {
    let slow_fs_ms = std::env::var("NAVO_SLOW_FS_MS").ok();
    let local: Arc<dyn ProviderOps> = Arc::new(LocalFs::new());
    let local = match SlowFs::parse_delay_ms(slow_fs_ms.as_deref()) {
        Some(delay) => Arc::new(SlowFs::new(local, delay)),
        None => local,
    };
    match trace {
        Some(trace) => trace.provider(local),
        None => local,
    }
}

#[expect(clippy::too_many_lines, reason = "one field per window service")]
pub(crate) fn open(mut launch: Launch) {
    let dirs = launch.dirs;
    let settings_start = settings::start_from(
        (dirs.as_ref(), launch.store.journal.clone()),
        launch.experimental_retention,
    );
    let (keymap, _keymap_watch) = keymap::hooks(dirs.as_ref());
    let home = home_location(|name| std::env::var(name).ok());
    let state = session_state(dirs.as_ref(), &home, launch.panes, &launch.splash);
    let local = local_provider(launch.trace.as_ref());
    let services = profile_services(
        (dirs.as_ref(), launch.store.journal.clone()),
        &home,
        launch.experimental_retention,
        &local,
    );
    let keys = CacheKeys::new(Arc::new(drive_identity));
    let (cache, disk) = listing_cache(dirs.as_ref(), &keys, &state);
    crate::diag::register_listings(&launch.crash.diagnostics.caches, &cache, disk.as_ref());
    metrics::mark("listing cache read");
    let remember = crate::search_hooks::Remember::default();
    let (mut settings, _settings_watch) =
        finish_settings(|| settings_start.finish().into_loaded(), &remember);
    settings::wire_diagnostics(&mut settings, &launch.crash, dirs.as_ref());
    settings::wire_guide_hint(&mut settings, dirs.as_ref());
    let (search, visits) = search_hooks(services.memory.clone(), remember, dirs.as_ref(), &local);
    let (save, autosave) = session_saver(dirs, disk.clone(), cache.clone(), keys.clone());
    let exit = (disk, visits, services.memory.clone());
    files::prewarm_pdf(PDF_WARM_DELAY);
    run(
        StartOptions {
            state,
            provider: local,
            on_save: save,
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
            crash_report: finish_startup(&mut launch.store, launch.crash, launch.view.is_some()),
            view: launch.view.and_then(|path| path_location(&path)),
            listing_cache: cache,
            cache_keys: keys,
            shell_menu: shell_menu::provider(),
            file_clipboard: Some(clipboard::service()),
            drop_platform: Some(crate::drag::service()),
            start_watch: launch.start_watch,
            open: open_hooks(launch.path, launch.instance_requests, launch.splash),
            rename: Some(services.rename),
            retained: services.retained,
        },
        Rc::new(rusty_shell::display_scale),
        RunHooks {
            on_quit: quitter(exit, launch.cleanup, launch.running, services.maintenance),
            on_autosave: autosave,
            trace: launch.trace,
        },
    );
}

fn search_hooks(
    memory: Option<files::SharedDb>,
    remember: crate::search_hooks::Remember,
    dirs: Option<&StoreDirs>,
    local: &Arc<dyn ProviderOps>,
) -> (rusty_ui::window_view::SearchHooks, Option<Arc<Visits>>) {
    let (mut search, visits) = crate::search_hooks::with_visits(memory, remember);
    search.index = index_hooks(dirs.cloned(), local.clone(), &mut search.service);
    crate::search_hooks::add_upgrade_notice(&mut search.service, dirs);
    (search, visits)
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

fn open_hooks(path: Option<PathBuf>, inbox: OpenInbox, splash: crate::splash::Splash) -> OpenHooks {
    OpenHooks {
        path: path.and_then(|path| path_location(&path)),
        inbox: Some(inbox),
        on_shown: splash.on_shown(),
    }
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
) -> (SaveSession, BackgroundSaveSession) {
    let session: BackgroundSaveSession = Arc::new(move |state: &AppState| {
        if let Some(dirs) = dirs.as_ref()
            && dirs.ensure().is_ok()
            && let Err(error) = save(state, &dirs.session_file())
        {
            tracing::warn!(%error, "saving the session failed");
        }
    });
    let autosave = session.clone();
    let on_close = Rc::new(move |state: &AppState| {
        session(state);
        if let Some(disk) = &disk {
            disk.save_folders(&cache, &keys, &session_folders(state));
            if !disk.flush(EXIT_WRITE) {
                tracing::warn!("listing cache writes skipped at exit");
            }
        }
    });
    (on_close, autosave)
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

#[cfg(test)]
mod tests;
