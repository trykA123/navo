use std::sync::Arc;

pub type CommandUse = Arc<dyn Fn() -> Vec<(String, f64)> + Send + Sync>;
pub type RecordCommand = Arc<dyn Fn(&str) + Send + Sync>;
pub type Frecents = Arc<dyn Fn(usize) -> Vec<Frecent> + Send + Sync>;
pub type RecordVisit = Arc<dyn Fn(&str, bool) + Send + Sync>;
pub type ClearSearch = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Debug, PartialEq)]
pub struct Frecent {
    pub location: String,
    pub is_folder: bool,
    pub weight: f64,
}

#[derive(Clone)]
pub struct SearchHooks {
    pub command_use: CommandUse,
    pub record_command: RecordCommand,
    pub frecent: Frecents,
    pub record_visit: RecordVisit,
    pub clear: ClearSearch,
}

impl SearchHooks {
    #[must_use]
    pub fn none() -> Self {
        Self {
            command_use: Arc::new(Vec::new),
            record_command: Arc::new(|_| {}),
            frecent: Arc::new(|_| Vec::new()),
            record_visit: Arc::new(|_, _| {}),
            clear: Arc::new(|| {}),
        }
    }
}
