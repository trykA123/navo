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
