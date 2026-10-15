use std::fs::File;
use std::rc::Rc;
use std::sync::Arc;

use rusty_core::{AppState, Location};
use rusty_fs::{LocalFs, ProviderOps};
use rusty_store::StoreDirs;
use rusty_ui::browser::Operations;
use rusty_ui::tokens::Mode;
use rusty_ui::window_view::{StartOptions, run};

use crate::startup::{home_location, initial_state, save};
use crate::{jobs, launch};

pub fn open() {
    let dirs = StoreDirs::detect();
    let session = dirs.as_ref().map(StoreDirs::session_file);
    let home = home_location(|name| std::env::var(name).ok());
    let state = session.as_deref().map_or_else(
        || initial_state(std::path::Path::new(""), home.clone()),
        |file| initial_state(file, home.clone()),
    );
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
    });
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
        return (unlogged, instance);
    };
    if instance.is_some() {
        let _ = jobs::recover(&journal, ops);
    }
    let logged = Operations {
        log: jobs::log_factory(journal),
        protected,
    };
    (logged, instance)
}

#[cfg(test)]
mod tests;
