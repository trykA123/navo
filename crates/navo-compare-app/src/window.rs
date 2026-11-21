//! The main window of Sidewise: starts the app and opens the themed shell.

use navo_compare_cli::args::SideSpec;
use navo_compare_profile::brand;
use sidewise_kit::prelude::*;
use sidewise_kit::tokens::Mode;

use crate::launch::Opening;
use crate::screens::compare_tab::{CompareRequest, install_compare_hooks};
use crate::shell::{self, Shell};
use crate::start_tabs::StartChoice;

/// Starts the GPUI app, opens the window from `opening` and returns the process exit code (0 after a normal close).
pub fn run(opening: Opening) -> std::process::ExitCode {
    let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let run_failed = failed.clone();
    let Opening { pair, profile } = opening;
    sidewise_kit::application().run(move |cx| {
        let initialised = sidewise_kit::init(Mode::Light, cx);
        shell::bind_keys(cx);
        crate::screens::compare_host::bind_compare_keys(cx);
        install_compare_hooks(cx);
        if let Err(message) = Shell::install_sessions(profile.as_deref(), cx) {
            report(&message);
        }
        if let Err(message) = crate::shell_prefs::install_prefs(profile.as_deref(), cx) {
            report(&message);
        }
        crate::updates::install_updates(profile.as_deref(), cx);
        crate::viewed_store::install_viewed(profile.as_deref());
        crate::screens::host_parts::install_hashes(profile.as_deref());
        let writes = crate::writes::install_writes(profile.as_deref());
        let opened = initialised
            .map_err(|error| error.to_string())
            .and_then(|()| open(cx, pair));
        match opened {
            Ok(()) => crate::updates_check::check_at_start(cx),
            Err(message) => {
                report(&message);
                run_failed.store(true, std::sync::atomic::Ordering::SeqCst);
                cx.quit();
            }
        }
        crate::writes::announce(writes, cx);
    });
    if failed.load(std::sync::atomic::Ordering::SeqCst) {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

/// Opens the 1,200 x 760 window titled with the product name, with `pair` on the first compare tab.
fn open(cx: &mut App, pair: Option<(SideSpec, SideSpec)>) -> Result<(), String> {
    let bounds = Bounds::centered(None, size(px(1200.), px(760.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some(brand::SLUG.into()),
        ..sidewise_kit::title_bar::window_options(brand::NAME)
    };
    sidewise_kit::open_window(options, cx, |window, cx| {
        let shell = cx.new(|cx| Shell::new(window, cx));
        shell.update(cx, |shell, cx| {
            shell.sync_rows(cx);
            if let Some((left, right)) = pair {
                let _ = shell.open_compare(
                    (
                        0,
                        CompareRequest {
                            left,
                            right,
                            choice: StartChoice::default(),
                        },
                    ),
                    window,
                    cx,
                );
            }
        });
        shell
    })
    .map(|_| ())
    .map_err(|error| format!("could not open the window: {error:#}"))
}

/// Prints a start-up failure on the terminal.
#[expect(clippy::print_stderr, reason = "start-up failures go to the terminal")]
fn report(message: &str) {
    eprintln!("{}: {message}", brand::SLUG);
}
