use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusty_ui::prelude::*;
use rusty_ui::tokens::Mode;

use crate::gallery::Gallery;
use crate::report;

const SETTLE: Duration = Duration::from_millis(1500);

fn grim(path: &Path) -> Result<(), String> {
    let status = Command::new("grim")
        .arg(path)
        .status()
        .map_err(|error| format!("running grim for {}: {error}", path.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("grim failed for {}: {status}", path.display()))
    }
}

pub fn capture(gallery: Entity<Gallery>, dir: PathBuf, failed: Arc<AtomicBool>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut errors = Vec::new();
        for mode in [Mode::Light, Mode::Dark] {
            gallery.update(cx, |gallery, cx| gallery.set_mode(mode, cx));
            cx.background_executor().timer(SETTLE).await;
            let name = match mode {
                Mode::Light => "p7-gallery-light.png",
                Mode::Dark => "p7-gallery-dark.png",
            };
            let path = dir.join(name);
            let shot = cx
                .background_executor()
                .spawn(async move { grim(&path) })
                .await;
            errors.extend(shot.err());
        }
        for error in &errors {
            report(error);
        }
        failed.store(!errors.is_empty(), Ordering::SeqCst);
        cx.update(|cx| cx.quit());
    })
    .detach();
}
