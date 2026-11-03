use std::path::{Path, PathBuf};

use navo_shell::Hive;

/// What detection may ask the machine. Reads only; nothing is spawned.
pub(crate) trait ToolEnv {
    fn windows(&self) -> bool;
    fn var(&self, name: &str) -> Option<String>;
    fn registry(&self, hive: Hive, key: &str, value: &str) -> Option<String>;
    fn is_file(&self, path: &Path) -> bool;
    fn on_path(&self, program: &str) -> Option<PathBuf>;
}

pub(crate) struct SystemEnv;

impl ToolEnv for SystemEnv {
    fn windows(&self) -> bool {
        cfg!(windows)
    }

    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|value| !value.is_empty())
    }

    fn registry(&self, hive: Hive, key: &str, value: &str) -> Option<String> {
        navo_shell::registry_text(hive, key, value)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn on_path(&self, program: &str) -> Option<PathBuf> {
        let search = crate::editor::Search {
            path: self.var("PATH")?,
            extensions: self
                .var("PATHEXT")
                .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned()),
        };
        crate::editor::resolve(program, &search)
    }
}
