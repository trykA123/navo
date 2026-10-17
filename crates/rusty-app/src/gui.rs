use std::fs::File;
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
use rusty_ui::window_view::{StartOptions, run};

use crate::startup::{home_location, initial_state, save, session_folders};
use crate::{app_menu, drives, editor, files, jobs, keymap, launch, settings, shell_menu};

const START_READ: Duration = Duration::from_millis(50);
const EXIT_WRITE: Duration = Duration::from_millis(300);

pub fn open(view: Option<std::path::PathBuf>) {
    let dirs = StoreDirs::detect();
    let session = dirs.as_ref().map(StoreDirs::session_file);
    let settings_start = settings::start_from(dirs.as_ref());
    let (keymap, _keymap_watch) = keymap::hooks(dirs.as_ref());
    let home = home_location(|name| std::env::var(name).ok());
    let no_session = std::path::Path::new("");
    let state = initial_state(session.as_deref().unwrap_or(no_session), home.clone());
    let memory = dirs
        .as_ref()
        .filter(|dirs| dirs.ensure().is_ok())
        .and_then(|dirs| files::open_memory(&dirs.data.join("ferrite.db")));
    let local = Arc::new(LocalFs::new());
    let (operations, _instance) = operations(dirs.as_ref(), &home, local.as_ref());
    let keys = CacheKeys::new(Arc::new(drive_identity));
    let (cache, disk) = listing_cache(dirs.as_ref(), &keys, &state);
    let (exit_cache, exit_keys) = (cache.clone(), keys.clone());
    let on_save = Rc::new(move |state: &AppState| {
        if let (Some(dirs), Some(file)) = (dirs.as_ref(), session.as_deref())
            && dirs.ensure().is_ok()
        {
            let _ = save(state, file);
        }
        if let Some(disk) = &disk {
            disk.save_folders(&exit_cache, &exit_keys, &session_folders(state));
            if !disk.flush(EXIT_WRITE) {
                tracing::warn!("listing cache writes skipped at exit");
            }
        }
    });
    let started = settings_start.finish();
    tracing::info!(viewer = view.is_some(), "window opening");
    run(StartOptions {
        state,
        provider: local,
        on_save,
        mode: Mode::Light,
        operations,
        launcher: launch::launcher(),
        editor: editor::hooks(|name| std::env::var(name).ok()),
        files: files::hooks(memory),
        kinds: rusty_ui::window_view::Kinds::builtin(),
        drives: drives::hooks(),
        app_menu: app_menu::hooks(std::env::current_exe().ok().as_deref()),
        settings: settings::hooks(started.loaded),
        keymap,
        view: view.and_then(|path| view_location(&path)),
        listing_cache: cache,
        cache_keys: keys,
        shell_menu: shell_menu::provider(),
    });
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

fn view_location(path: &std::path::Path) -> Option<Location> {
    let path = std::path::absolute(path).ok()?;
    Location::new(path.to_str()?).ok()
}

pub fn operations(
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
    let journal = dirs
        .ensure()
        .ok()
        .and_then(|()| jobs::open(&dirs.data.join("ferrite.db")).ok());
    let instance = jobs::claim(&dirs.data.join("ferrite.lock"));
    let Some(journal) = journal else {
        let unlogged = Operations {
            protected,
            ..Operations::unlogged()
        };
        return (unlogged, instance.and_then(jobs::Instance::share));
    };
    if instance.as_ref().is_some_and(|instance| instance.sole) {
        let _ = jobs::recover(&journal, ops);
    }
    let logged = Operations {
        log: jobs::log_factory(journal),
        protected,
    };
    (logged, instance.and_then(jobs::Instance::share))
}

#[cfg(test)]
mod tests;
