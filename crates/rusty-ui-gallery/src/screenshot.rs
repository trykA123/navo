use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusty_ui::prelude::*;
use rusty_ui::tokens::Mode;

use crate::gallery::{Gallery, Page};
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

fn file_name(page: Page, mode: Mode) -> String {
    let page = match page {
        Page::Primitives => "p7-gallery",
        Page::Pane => "p7-pane",
        Page::Operate => "p8-confirm",
        Page::Hex => "p13-hex",
        Page::PagedText => "p13-text",
        Page::Drives => "p14-drives",
        Page::TypedPath => "p14-path",
    };
    let mode = match mode {
        Mode::Light => "light",
        Mode::Dark => "dark",
    };
    format!("{page}-{mode}.png")
}

pub fn capture(gallery: Entity<Gallery>, dir: PathBuf, failed: Arc<AtomicBool>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut errors = Vec::new();
        for (page, mode) in Page::ALL
            .into_iter()
            .flat_map(|page| [Mode::Light, Mode::Dark].map(|mode| (page, mode)))
        {
            gallery.update(cx, |gallery, cx| {
                gallery.set_page(page, cx);
                gallery.set_mode(mode, cx);
            });
            cx.background_executor().timer(SETTLE).await;
            let path = dir.join(file_name(page, mode));
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
