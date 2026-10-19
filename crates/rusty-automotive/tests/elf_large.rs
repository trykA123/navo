mod elf_build;

use std::time::Instant;

use elf_build::{Named, Spec, build, data, func, object, read, text};

const SYMBOLS: usize = 50_000;

fn large() -> Vec<u8> {
    let mut symbols: Vec<Named> = (0..SYMBOLS)
        .map(|at| {
            let address = 0x0800_0000 + (at as u64) * 200;
            func(
                &format!("module_{:03}_function_{at}", at % 997),
                address,
                200,
                0,
            )
        })
        .collect();
    symbols.extend((0..SYMBOLS / 10).map(|at| {
        object(
            &format!("_ZN6driver5state{at}E"),
            0x2000_0000 + at as u64 * 4,
            4,
            1,
        )
    }));
    let debug_rows: Vec<(u64, u64, u64)> = (0..SYMBOLS as u64)
        .map(|at| (0x0800_0000 + at * 200, 200, 10 + at % 500))
        .collect();
    let spec = Spec {
        is_64: false,
        is_big: true,
        machine: elf_build::EM_V800,
        entry: 0x0800_0000,
        parts: vec![
            text(0x0800_0000, SYMBOLS * 200),
            data(0x2000_0000, SYMBOLS / 10 * 4),
        ],
        symbols: Some(symbols),
        loads: vec![(0, 5), (1, 6)],
        debug: elf_build::line_program(false, true, &debug_rows),
    };
    build(&spec)
}

#[test]
fn a_large_firmware_with_fifty_thousand_symbols_and_ten_megabytes_reads_quickly() {
    let bytes = large();
    assert!(bytes.len() > 10_000_000, "{}", bytes.len());
    let started = Instant::now();
    let (elf, lines) = read(&bytes).unwrap();
    let took = started.elapsed();
    assert_eq!(elf.symbols.len(), SYMBOLS + SYMBOLS / 10);
    assert_eq!(lines.found(), SYMBOLS);
    assert!(took.as_secs() < 10, "{took:?}");
}
