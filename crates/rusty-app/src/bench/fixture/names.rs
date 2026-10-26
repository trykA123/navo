const WORDS: [&str; 20] = [
    "brake", "motor", "sensor", "calib", "boot", "flash", "trace", "can", "lin", "ecu", "diag",
    "report", "build", "config", "module", "driver", "signal", "stack", "timer", "adc",
];
const UNICODE_WORDS: [&str; 10] = [
    "Größe",
    "señal",
    "日本語",
    "データ",
    "Ölstand",
    "naïve",
    "Ελληνικά",
    "测试",
    "Привод",
    "café",
];
const EXTENSIONS: [&str; 24] = [
    "c", "c", "c", "h", "h", "o", "elf", "map", "a2l", "dbc", "txt", "txt", "pdf", "png", "json",
    "xml", "log", "hex", "mdf", "s", "cpp", "md", "zip", "",
];
const LONG_NAME_DEPTH: usize = 3;

pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub(super) fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    fn percent(&mut self) -> usize {
        self.below(100)
    }
}

fn word(rng: &mut Rng) -> &'static str {
    if rng.percent() < 5 {
        UNICODE_WORDS[rng.below(UNICODE_WORDS.len())]
    } else {
        WORDS[rng.below(WORDS.len())]
    }
}

fn with_extension(stem: String, rng: &mut Rng) -> String {
    match EXTENSIONS[rng.below(EXTENSIONS.len())] {
        "" => stem,
        ext => format!("{stem}.{ext}"),
    }
}

pub(super) fn folder_name(rng: &mut Rng, index: usize) -> String {
    format!("{}-{index}", word(rng))
}

pub(super) fn file_name(seed: u64, index: usize, depth: usize) -> String {
    let mut rng = Rng::new(seed ^ (index as u64).wrapping_mul(0xA24B_AED4_963E_E407));
    let stem = if depth <= LONG_NAME_DEPTH && rng.percent() < 2 {
        let words: Vec<_> = (0..14).map(|_| word(&mut rng)).collect();
        format!("{}_{index}", words.join("_"))
    } else {
        format!("{}_{}_{index}", word(&mut rng), word(&mut rng))
    };
    with_extension(stem, &mut rng)
}

pub(super) fn huge_name(seed: u64, index: usize) -> String {
    let mut rng = Rng::new(seed ^ (index as u64).wrapping_mul(0xA24B_AED4_963E_E407));
    with_extension(format!("{}_{index:06}", word(&mut rng)), &mut rng)
}
