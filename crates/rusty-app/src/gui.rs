use std::fs::File;
use std::rc::Rc;
use std::sync::Arc;

use rusty_cache::DriveIdentity;
use rusty_core::{AppState, Location};
use rusty_fs::{LocalFs, ProviderOps};
use rusty_shell::VolumeIdentity;
use rusty_store::StoreDirs;
use rusty_ui::browser::Operations;
use rusty_ui::tokens::Mode;
use rusty_ui::window_view::{StartOptions, run};

use crate::startup::{home_location, initial_state, save};
use crate::{drives, editor, files, jobs, launch, shell_menu};

pub fn open(view: Option<std::path::PathBuf>) {
    let dirs = StoreDirs::detect();
    let session = dirs.as_ref().map(StoreDirs::session_file);
    let settings = dirs.as_ref().map(StoreDirs::settings_file);
    let home = home_location(|name| std::env::var(name).ok());
    let state = session.as_deref().map_or_else(
        || initial_state(std::path::Path::new(""), home.clone()),
        |file| initial_state(file, home.clone()),
    );
    let memory = dirs
        .as_ref()
        .filter(|dirs| dirs.ensure().is_ok())
        .and_then(|dirs| files::open_memory(&dirs.data.join("ferrite.db")));
    let local = Arc::new(LocalFs::new());
    let (operations, _instance) = operations(dirs.as_ref(), &home, local.as_ref());
    let on_save = Rc::new(move |state: &AppState| {
        if let (Some(dirs), Some(file)) = (dirs.as_ref(), session.as_deref())
            && dirs.ensure().is_ok()
        {
            let _ = save(state, file);
        }
    });
    run(StartOptions {
        state,
        provider: local,
        on_save,
        mode: Mode::Light,
        operations,
        launcher: launch::launcher(),
        editor: editor::hooks(settings.as_deref(), |name| std::env::var(name).ok()),
        files: files::hooks(memory),
        kinds: rusty_ui::window_view::Kinds::builtin(),
        drives: drives::hooks(),
        view: view.and_then(|path| view_location(&path)),
        listing_cache: rusty_cache::ListingCache::default(),
        cache_keys: rusty_cache::CacheKeys::new(Arc::new(drive_identity)),
        shell_menu: shell_menu::provider(),
    });
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
