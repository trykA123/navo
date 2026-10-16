use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use rusty_core::Location;
use rusty_core::shell_menu::{ItemId, ShellMenu, Submenu};
use rusty_shell::{
    InvokeOptions, MenuSession, Pending, ScreenPoint, ShellMenuService, Show, WindowHandle,
};
use rusty_ui::window_view::{Reply, ScreenSpot, ShellMenuProvider, ShellMenuSession};

pub fn provider() -> Option<Rc<dyn ShellMenuProvider>> {
    let service = ShellMenuService::start().ok()?;
    Some(Rc::new(Provider(service)))
}

struct Provider(ShellMenuService);

struct Session(MenuSession);

fn reply<T: 'static, U>(pending: Pending<T>, map: impl Fn(T) -> U + 'static) -> Reply<U> {
    Box::new(move || {
        let answer = pending.wait_timeout(Duration::ZERO)?;
        Some(answer.map(&map).map_err(|error| error.to_string()))
    })
}

fn options(at: ScreenSpot) -> InvokeOptions {
    InvokeOptions {
        hwnd: at.window.map(WindowHandle),
        point: Some(ScreenPoint { x: at.x, y: at.y }),
        show: Show::Normal,
    }
}

impl ShellMenuProvider for Provider {
    fn open(&self, items: &[Location]) -> Reply<Box<dyn ShellMenuSession>> {
        let paths = items
            .iter()
            .map(|item| PathBuf::from(item.as_str()))
            .collect();
        reply(self.0.open(paths), |session| {
            Box::new(Session(session)) as Box<dyn ShellMenuSession>
        })
    }
}

impl ShellMenuSession for Session {
    fn menu(&self) -> &ShellMenu {
        self.0.menu()
    }

    fn fill(&self, path: &[usize]) -> Reply<Submenu> {
        reply(self.0.fill(path), |submenu| submenu)
    }

    fn invoke(&self, id: &ItemId, at: ScreenSpot) -> Reply<()> {
        reply(self.0.invoke(id, options(at)), |()| ())
    }

    fn classic(&self, at: ScreenSpot) -> Reply<()> {
        let window = WindowHandle(at.window.unwrap_or_default());
        let point = ScreenPoint { x: at.x, y: at.y };
        reply(self.0.classic(window, point), |_| ())
    }
}

#[cfg(test)]
mod tests;
