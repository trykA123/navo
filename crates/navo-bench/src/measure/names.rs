const WORDS: [&str; 16] = [
    "report", "budget", "design", "survey", "draft", "invoice", "notes", "photo", "summary",
    "plan", "review", "export", "backup", "sample", "record", "outline",
];
const EXTENSIONS: [&str; 12] = [
    "txt", "md", "log", "csv", "json", "xml", "pdf", "png", "jpg", "zip", "c", "rs",
];
const SENSOR_EVERY: usize = 50;

/// Seeded generator of file names; the same seed gives the same names.
pub(crate) struct Names {
    state: u64,
}

impl Names {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> usize {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.state >> 33).unwrap_or(0)
    }

    fn pick(&mut self, list: &[&'static str]) -> &'static str {
        let at = self.next() % list.len().max(1);
        list.get(at).copied().unwrap_or("item")
    }

    /// A unique name for position `index`: word, number, extension; every 50th says `sensor`.
    pub(crate) fn file(&mut self, index: usize) -> String {
        let word = if index.is_multiple_of(SENSOR_EVERY) {
            "sensor"
        } else {
            self.pick(&WORDS)
        };
        let extension = self.pick(&EXTENSIONS);
        format!("{word}-{index:05}.{extension}")
    }

    /// A unique folder name for position `index`.
    pub(crate) fn folder(&mut self, index: usize) -> String {
        let word = self.pick(&WORDS);
        format!("{word}-{index:03}")
    }
}
