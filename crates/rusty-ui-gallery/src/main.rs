#![forbid(unsafe_code)]

mod automotive_sample;
mod columns_page;
mod drives_page;
mod gallery;
mod image_sample;
mod job_sample;
mod kinds_page;
mod map_sample;
mod menu_page;
mod menu_sample;
mod mouse_page;
mod operate_page;
mod options;
mod pages;
mod pane_page;
mod pane_sample;
mod pdf_sample;
mod rich_sample;
mod screenshot;
mod sections;
mod settings_sample;
mod sheet_sample;
mod shortcuts_sample;
mod structured_sample;
mod tokens_page;
mod tokens_sample;
mod viewer_menu_page;
mod viewer_sample;
mod window_page;
mod window_samples;

#[cfg(test)]
mod tests;

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gallery::Gallery;
use rusty_ui::prelude::*;
use rusty_ui::tokens::Mode;

#[allow(
    clippy::print_stderr,
    reason = "the gallery reports failures on the terminal"
)]
fn report(message: &str) {
    eprintln!("{message}");
}

fn window_options(cx: &App) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("ferrite-gallery".into()),
        ..rusty_ui::window_options("Ferrite components")
    }
}

fn main() -> ExitCode {
    let options = match options::parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            report(&format!("{message}\n{}", options::USAGE));
            return ExitCode::from(2);
        }
    };
    let failed = Arc::new(AtomicBool::new(false));
    let run_failed = failed.clone();
    rusty_ui::application().run(move |cx| {
        if let Err(error) = rusty_ui::init(Mode::Light, cx) {
            report(&error.to_string());
        }
        rusty_ui::confirm_dialog::bind_keys(cx);
        rusty_ui::prompt_dialog::bind_keys(cx);
        rusty_ui::context_menu::bind_keys(cx);
        rusty_ui::pane_chrome::bind_keys(cx);
        rusty_ui::window_view::bind_viewer_keys(cx);
        let opened = rusty_ui::open_window(window_options(cx), cx, |window, cx| {
            cx.new(|cx| Gallery::new(Mode::Light, window, cx))
        });
        match (opened, options.screenshot_dir) {
            (Err(error), _) => {
                report(&format!("could not open the gallery window: {error:#}"));
                run_failed.store(true, Ordering::SeqCst);
                cx.quit();
            }
            (Ok((window, gallery)), Some(dir)) => {
                let shot = screenshot::Shot {
                    window,
                    gallery,
                    dir,
                };
                screenshot::capture(shot, run_failed, cx);
            }
            (Ok(_), None) => {}
        }
    });
    if failed.load(Ordering::SeqCst) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
