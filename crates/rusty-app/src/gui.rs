use std::rc::Rc;
use std::sync::Arc;

use rusty_core::AppState;
use rusty_fs::LocalFs;
use rusty_store::StoreDirs;
use rusty_ui::tokens::Mode;
use rusty_ui::window_view::{StartOptions, run};

use crate::startup::{home_location, initial_state, save};

pub fn open() {
    let dirs = StoreDirs::detect();
    let session = dirs.as_ref().map(StoreDirs::session_file);
    let home = home_location(|name| std::env::var(name).ok());
    let state = session.as_deref().map_or_else(
        || initial_state(std::path::Path::new(""), home.clone()),
        |file| initial_state(file, home.clone()),
    );
    let on_save = Rc::new(move |state: &AppState| {
        if let (Some(dirs), Some(file)) = (dirs.as_ref(), session.as_deref())
            && dirs.ensure().is_ok()
        {
            let _ = save(state, file);
        }
    });
    run(StartOptions {
        state,
        provider: Arc::new(LocalFs::new()),
        on_save,
        mode: Mode::Light,
    });
}
