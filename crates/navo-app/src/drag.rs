use std::path::PathBuf;
use std::rc::Rc;

use navo_core::Location;
use navo_ui::window_view::DropPlatform;

use crate::shell_menu::without_verbatim;
use crate::startup::path_location;

pub fn service() -> Rc<dyn DropPlatform> {
    #[cfg(windows)]
    gpui_windows::set_external_drag_handler(drag_files);
    Rc::new(ShellDrag)
}

struct ShellDrag;

impl DropPlatform for ShellDrag {
    #[cfg(windows)]
    fn keys(&self, window: Option<isize>) -> Option<navo_core::DropKeys> {
        Some(navo_core::DropKeys::from_key_state(
            gpui_windows::drag_key_state(window?),
        ))
    }

    #[cfg(unix)]
    fn keys(&self, _window: Option<isize>) -> Option<navo_core::DropKeys> {
        Some(navo_core::DropKeys {
            ctrl: true,
            ..navo_core::DropKeys::default()
        })
    }

    #[cfg(unix)]
    fn allows(&self, _window: Option<isize>, op: navo_core::DropOp) -> bool {
        op == navo_core::DropOp::Copy
    }

    #[cfg(windows)]
    fn right_button(&self, window: Option<isize>) -> bool {
        window.is_some_and(|window| {
            navo_core::right_button_drag(gpui_windows::drag_key_state(window))
        })
    }

    #[cfg(windows)]
    fn allows(&self, window: Option<isize>, op: navo_core::DropOp) -> bool {
        window.is_some_and(|window| gpui_windows::drag_allowed_effects(window) & effect(op) != 0)
    }

    #[cfg(windows)]
    fn set_effect(&self, window: Option<isize>, op: Option<navo_core::DropOp>) {
        if let Some(window) = window {
            gpui_windows::set_drop_effect(window, Some(op.map_or(0, effect)));
        }
    }

    fn locations(&self, paths: &[PathBuf]) -> Result<Vec<Location>, String> {
        paths
            .iter()
            .map(|path| {
                if !path.is_absolute() {
                    return Err(navo_ui::window_view::invalid_drop_path(path));
                }
                path_location(path).ok_or_else(|| navo_ui::window_view::invalid_drop_path(path))
            })
            .collect()
    }

    fn paths(&self, items: &[(Location, bool)]) -> Option<Vec<(PathBuf, bool)>> {
        items
            .iter()
            .map(|(item, folder)| {
                let path = navo_fs::os_path(item).ok()?;
                Some((
                    PathBuf::from(without_verbatim(path.into_os_string())),
                    *folder,
                ))
            })
            .collect()
    }
}

#[cfg(windows)]
fn effect(op: navo_core::DropOp) -> u32 {
    match op {
        navo_core::DropOp::Copy => 1,
        navo_core::DropOp::Move => 2,
        navo_core::DropOp::Link => 4,
    }
}

#[cfg(windows)]
fn drag_files(window: isize, files: &[(PathBuf, bool)]) -> u32 {
    let paths: Vec<_> = files.iter().map(|(path, _)| path.clone()).collect();
    match navo_shell::drag_files(window, &paths) {
        Ok(navo_shell::DropOutcome::Copied) => 1,
        Ok(navo_shell::DropOutcome::Moved) => 2,
        Ok(navo_shell::DropOutcome::Linked) => 4,
        Ok(navo_shell::DropOutcome::NoEffect) => 0,
        Err(error) => {
            tracing::warn!(%error, "native file drag failed");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn incoming_unix_drops_are_copy_only_without_native_modifiers() {
        use navo_core::{DropKeys, DropOp};
        assert_eq!(
            ShellDrag.keys(None),
            Some(DropKeys {
                ctrl: true,
                ..DropKeys::default()
            })
        );
        assert!(ShellDrag.allows(None, DropOp::Copy));
        assert!(!ShellDrag.allows(None, DropOp::Move));
        assert!(!ShellDrag.allows(None, DropOp::Link));
    }

    #[test]
    fn relative_external_paths_are_refused() {
        assert!(
            ShellDrag
                .locations(&[PathBuf::from("relative.txt")])
                .is_err()
        );
    }

    #[test]
    fn nonlocal_sources_offer_no_partial_native_payload() {
        let local = path_location(&std::env::temp_dir()).expect("local temp");
        let archive = Location::new("zip:///tmp/a.zip!/file.txt").expect("archive");
        assert!(
            ShellDrag
                .paths(&[(local, true), (archive, false)])
                .is_none()
        );
    }
}
