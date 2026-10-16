use std::path::PathBuf;

use rusty_core::shell_menu::{ShellMenu, Submenu};

use super::{InvokeOptions, MenuError, MenuSession, Pending, ScreenPoint, WindowHandle};

pub(super) struct Service;

impl Service {
    #[expect(clippy::unnecessary_wraps, reason = "matches the Windows service")]
    pub(super) fn start() -> Result<Self, MenuError> {
        Ok(Self)
    }

    pub(super) fn open(&self, _paths: Vec<PathBuf>) -> Pending<MenuSession> {
        Pending::ready(Ok(MenuSession {
            menu: ShellMenu::default(),
            handle: SessionHandle,
        }))
    }
}

pub(super) struct SessionHandle;

impl SessionHandle {
    pub(super) fn fill(&self, _path: Vec<usize>) -> Pending<Submenu> {
        Pending::ready(Err(MenuError::Unsupported))
    }

    pub(super) fn invoke(
        &self,
        _command: u32,
        _verb: Option<String>,
        _options: InvokeOptions,
    ) -> Pending<()> {
        Pending::ready(Err(MenuError::Unsupported))
    }

    pub(super) fn classic(&self, _hwnd: WindowHandle, _point: ScreenPoint) -> Pending<Option<u32>> {
        Pending::ready(Err(MenuError::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rusty_core::shell_menu::{FerriteAction, ItemId};

    use crate::shell_menu::{
        InvokeOptions, MenuError, MenuSession, ScreenPoint, ShellMenuService, WindowHandle,
    };

    fn session() -> MenuSession {
        let service = ShellMenuService::start().expect("the stub service always starts");
        service
            .open(vec!["/data/a.txt".into(), "/data/b.txt".into()])
            .wait()
            .expect("the stub opens any one-folder selection")
    }

    #[test]
    fn open_gives_a_menu_without_shell_items() {
        assert!(session().menu().items.is_empty());
    }

    #[test]
    fn open_rejects_items_from_two_folders() {
        let service = ShellMenuService::start().expect("the stub service always starts");
        let result = service
            .open(vec!["/data/a.txt".into(), "/other/b.txt".into()])
            .wait();
        assert_eq!(result.err(), Some(MenuError::DifferentFolders));
    }

    #[test]
    fn invoke_is_unsupported() {
        let id = ItemId::Shell {
            command: 1,
            verb: Some("open".to_owned()),
        };
        let result = session().invoke(&id, InvokeOptions::default()).wait();
        assert_eq!(result, Err(MenuError::Unsupported));
    }

    #[test]
    fn invoking_a_ferrite_action_is_refused() {
        let id = ItemId::Ferrite(FerriteAction::CopyPath);
        let result = session().invoke(&id, InvokeOptions::default()).wait();
        assert_eq!(result, Err(MenuError::NotShellCommand));
    }

    #[test]
    fn classic_is_unsupported() {
        let pending = session().classic(WindowHandle(0), ScreenPoint::default());
        let result = pending.wait_timeout(Duration::from_secs(1));
        assert_eq!(result, Some(Err(MenuError::Unsupported)));
    }

    #[test]
    fn fill_is_unsupported() {
        assert_eq!(session().fill(&[0]).wait(), Err(MenuError::Unsupported));
    }
}
