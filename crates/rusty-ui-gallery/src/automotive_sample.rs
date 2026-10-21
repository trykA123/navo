use std::sync::LazyLock;

pub const MEMORY_NAME: &str = "bcm_app.hex";

pub const MEMORY_STEPS: [&str; 5] = [
    "viewer::LeftDrawer",
    "viewer::RowDown",
    "viewer::RowDown",
    "memory_image::PickItem",
    "viewer::RightDrawer",
];

static MEMORY: LazyLock<String> = LazyLock::new(memory_image);

pub const ELF_NAME: &str = "bcm_app.elf";

pub const ELF_STEPS: [&str; 4] = [
    "viewer::LeftDrawer",
    "viewer::RowDown",
    "elf_view::PickItem",
    "viewer::RightDrawer",
];

static ELF: LazyLock<Vec<u8>> =
    LazyLock::new(|| rusty_automotive::elf_sample::demo().unwrap_or_default());

pub fn elf() -> &'static [u8] {
    &ELF
}

pub fn memory() -> &'static str {
    &MEMORY
}

fn record(kind: u8, address: u16, data: &[u8]) -> String {
    let mut bytes = vec![data.len() as u8];
    bytes.extend(address.to_be_bytes());
    bytes.push(kind);
    bytes.extend(data);
    let sum = bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    bytes.push(0u8.wrapping_sub(sum));
    let digits: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    format!(":{digits}")
}

fn code(count: usize) -> Vec<u8> {
    let mut state = 0x2F6C_00E5u32;
    (0..count)
        .map(|at| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match at % 7 {
                3 => 0x00,
                6 => 0x4E,
                _ => (state >> 24) as u8,
            }
        })
        .collect()
}

fn memory_image() -> String {
    let header = b"BCM-GEN3 app header v2.4.1 \x00\x00\x00\x01 flash 0x08000400";
    let mut table = header.to_vec();
    table.resize(256, 0xFF);
    let mut lines = vec![record(4, 0, &[0x08, 0x00])];
    for (at, chunk) in table.chunks(16).enumerate() {
        lines.push(record(0, (at * 16) as u16, chunk));
    }
    for (at, chunk) in code(0x2000).chunks(16).enumerate() {
        lines.push(record(0, 0x400 + (at * 16) as u16, chunk));
    }
    if let Some(line) = lines.get_mut(60) {
        line.truncate(line.len() - 2);
        line.push_str("00");
    }
    lines.push(record(5, 0, &[0x08, 0x00, 0x04, 0x01]));
    lines.push(record(1, 0, &[]));
    lines.join("\n") + "\n"
}

pub const TRACE_NAME: &str = "body_can_2026-10-19.asc";

pub const TRACE_STEPS: [&str; 4] = [
    "viewer::LeftDrawer",
    "viewer::RowDown",
    "can_trace::PickId",
    "viewer::RightDrawer",
];

static TRACE: LazyLock<String> = LazyLock::new(can_trace);

pub fn trace() -> &'static str {
    &TRACE
}

const MESSAGES: [(&str, u8, u64); 6] = [
    ("0C9", 1, 10),
    ("1A0", 1, 20),
    ("3E9", 1, 50),
    ("18FEF100x", 2, 100),
    ("7DF", 2, 0),
    ("4F1", 1, 25),
];

fn frame_line(ms: u64, (id, channel): (&str, u8), data: &[u8]) -> String {
    let bytes: Vec<String> = data.iter().map(|byte| format!("{byte:02X}")).collect();
    format!(
        "{:>4}.{:06} {channel}  {id:<15} Rx   d {} {}",
        ms / 1000,
        ms % 1000 * 1000,
        data.len(),
        bytes.join(" ")
    )
}

fn can_trace() -> String {
    let mut lines = vec![
        "date Mon Oct 19 09:41:07.000 am 2026".to_owned(),
        "base hex  timestamps absolute".to_owned(),
        "internal events logged".to_owned(),
        "Begin Triggerblock Mon Oct 19 09:41:07.000 am 2026".to_owned(),
        "   0.000000 Start of measurement".to_owned(),
    ];
    let mut state = 0x5EED_1234u32;
    for ms in (0..1200u64).step_by(5) {
        for (id, channel, every) in MESSAGES {
            if every == 0 || ms % every != 0 {
                continue;
            }
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let len = if id == "3E9" { 4 } else { 8 };
            let data: Vec<u8> = (0..len).map(|at| (state >> (at % 4 * 8)) as u8).collect();
            lines.push(frame_line(ms, (id, channel), &data));
        }
        if ms == 400 {
            lines.push("   0.400500 2  ErrorFrame".to_owned());
        }
        if ms == 600 {
            lines.push(frame_line(
                600,
                ("7DF", 2),
                &[0x02, 0x01, 0x0D, 0, 0, 0, 0, 0],
            ));
            lines.push("   0.600800 CANFD   1 Rx        2A0  DiagFd   1 0 9 12 10 11 12 13 14 15 16 17 18 19 1A 1B   130000  0 1000 0 0 0 0 0".to_owned());
        }
    }
    lines.push("End TriggerBlock".to_owned());
    lines.join("\n") + "\n"
}

pub const DEMO_NAME: &str = "demo.mdf";

const DEMO_MDF: &str = "\
# invented memory definition for the gallery
space linear 1 8
space local 2 8
chip mpe:pflash0 10 8 0x200000 rom
chip mpe:pflash1 11 8 0x100000 rom
chip mpe:dflash0 12 8 0x10000 rom
chip mpe:dsram0 20 8 0x20000 ram
chip mpe:psram0 21 8 0x10000 ram
chip mpe:spare 30 8 0x1000 ram
map 1 0x80000000 0x200000 10 0
map 1 0x80200000 0x100000 11 0
map 1 0xAF000000 0x10000 12 0
map 1 0x70000000 0x20000 20 0
map 1 0x70100000 0x10000 21 0
map 2 0xD0000000 0x20000 20 0
map 2 0xC0000000 0x4000 99 0
map 1 0x80100000 0x1000 10 zero
";

pub fn demo_mdf() -> &'static str {
    DEMO_MDF
}
