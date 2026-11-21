//! Entry point of the app (its name is in `navo_compare_profile::brand`). `launch::decide` reads the
//! command line: it prints `--version` or `--help`, refuses an unsupported call, or opens the window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_mutex;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "U13c wires the chrome into the shell")
)]
mod chrome;
#[cfg(test)]
mod compare_tab_hex_width_tests;
#[cfg(test)]
mod compare_tab_keys_tests;
#[cfg(test)]
mod compare_tab_large_tests;
#[cfg(test)]
mod compare_tab_layout_tests;
#[cfg(test)]
mod compare_tab_menu_tests;
#[cfg(test)]
mod compare_tab_pair_tests;
#[cfg(test)]
mod compare_tab_script;
#[cfg(test)]
mod compare_tab_tests;
#[cfg(test)]
mod enter_keys_tests;
mod frame_notify;
#[cfg_attr(not(test), expect(dead_code, reason = "U11c and the chrome read it"))]
mod icons;
mod launch;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the drawer host stays unmounted (Navo's CompareScreen owns the diff drawers, NC119)"
    )
)]
mod layers;
mod menu_host;
#[cfg(test)]
mod menu_host_tests;
#[cfg_attr(not(test), expect(dead_code, reason = "the U13 chrome reads it"))]
mod motion;
#[cfg(test)]
mod motion_tests;
#[cfg_attr(not(test), expect(dead_code, reason = "H2 mounts the compare tab"))]
mod screens;
mod shell;
#[cfg(test)]
mod shell_git_refresh_tests;
#[cfg(test)]
mod shell_git_tests;
#[cfg(test)]
mod shell_last_tab_tests;
#[cfg(test)]
mod shell_layers_tests;
mod shell_prefs;
#[cfg(test)]
mod shell_prefs_tests;
#[cfg(test)]
mod shell_tests;
mod start;
mod start_focus;
mod start_git;
#[cfg(test)]
mod start_keys_tests;
mod start_menu;
#[cfg(test)]
mod start_menu_tests;
#[cfg(test)]
mod start_remote_tests;
mod start_rows;
#[cfg(test)]
mod start_rows_tests;
mod start_tabs;
#[cfg(test)]
mod start_tests;
mod start_updates;
mod start_view;
#[cfg(test)]
mod test_window;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "U13c wires the injector into the shell")
)]
mod theme;
#[cfg(test)]
mod theme_bridge_tests;
#[expect(dead_code, reason = "U14 hero art reads the remaining tokens")]
mod theme_data;
#[cfg(test)]
mod theme_tests;
#[cfg(test)]
mod ui_matrix_tests;
mod updates;
mod updates_check;
mod viewed_store;
#[cfg(test)]
mod viewed_store_tests;
mod window;
mod writes;
#[cfg(test)]
mod writes_hooks_tests;
#[cfg(test)]
mod writes_recovery_tests;
#[cfg(test)]
mod writes_tests;
#[cfg(test)]
mod writes_undo_tests;

use std::process::ExitCode;

use launch::Launch;
use navo_compare_cli::args::Exit;

/// Runs the command line: prints `--version` or `--help`, refuses what the alpha cannot serve
/// (exit code 2, before any window), or opens the window and returns its exit code.
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line output: version, help and refusals"
)]
fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    match launch::decide(&args, &cwd, &|path| path.exists()) {
        Launch::Print(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Launch::Refuse(text) => {
            eprintln!("{text}");
            ExitCode::from(Exit::Error as u8)
        }
        Launch::Open(opening) => {
            if let Err(error) = app_mutex::hold() {
                eprintln!("the installer cannot see this window: {error}");
            }
            window::run(opening)
        }
    }
}
