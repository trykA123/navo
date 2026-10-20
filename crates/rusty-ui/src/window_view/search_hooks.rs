use std::sync::Arc;

pub type CommandUse = Arc<dyn Fn() -> Vec<(String, f64)> + Send + Sync>;
pub type RecordCommand = Arc<dyn Fn(&str) + Send + Sync>;
pub type ClearSearch = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct SearchHooks {
    pub command_use: CommandUse,
    pub record_command: RecordCommand,
    pub clear: ClearSearch,
}

impl SearchHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            command_use: Arc::new(Vec::new),
            record_command: Arc::new(|_| {}),
            clear: Arc::new(|| {}),
        }
    }
}
