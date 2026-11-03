use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::{GitError, GitSource};

type Factory = dyn Fn(&Path) -> Result<Box<dyn GitSource>, GitError> + Send + Sync;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInfo {
    pub id: String,
    pub label: String,
}

#[derive(Clone)]
pub struct Engine {
    info: EngineInfo,
    factory: Arc<Factory>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl Engine {
    #[must_use]
    pub fn info(&self) -> &EngineInfo {
        &self.info
    }

    pub fn open(&self, repo: &Path) -> Result<Box<dyn GitSource>, GitError> {
        (self.factory)(repo)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EngineRegistry {
    engines: Vec<Engine>,
}

impl EngineRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&mut self, id: &str, label: &str, factory: F) -> Result<(), GitError>
    where
        F: Fn(&Path) -> Result<Box<dyn GitSource>, GitError> + Send + Sync + 'static,
    {
        if self.engines.iter().any(|engine| engine.info.id == id) {
            return Err(GitError::DuplicateEngine { id: id.to_owned() });
        }
        self.engines.push(Engine {
            info: EngineInfo {
                id: id.to_owned(),
                label: label.to_owned(),
            },
            factory: Arc::new(factory),
        });
        Ok(())
    }

    pub fn list(&self) -> impl Iterator<Item = &EngineInfo> {
        self.engines.iter().map(Engine::info)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Engine> {
        self.engines.iter().find(|engine| engine.info.id == id)
    }
}
