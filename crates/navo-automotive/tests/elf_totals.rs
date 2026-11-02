mod elf_build;

use elf_build::{Named, Part, build, firmware, func, named, parse, read, rh850, text};
use object::elf as raw;

#[test]
fn an_rwx_flash_segment_does_not_count_as_ram() {
    let mut spec = firmware(false, false);
    spec.parts.push(Part {
        name: ".init_array",
        flags: u64::from(raw::SHF_ALLOC | raw::SHF_WRITE),
        ..text(0x0800_0400 + 0x40, 0x10)
    });
    spec.loads[0].1 = raw::PF_R | raw::PF_W | raw::PF_X;
    spec.loads.push((3, raw::PF_R | raw::PF_W | raw::PF_X));
    let elf = parse(&build(&spec));
    assert_eq!(elf.totals.load, 0x400 + 0x40 + 0x10);
    assert_eq!(elf.totals.ram, 0x40 + 0x200);
}

#[test]
fn arm_mapping_symbols_are_left_out() {
    let mut spec = firmware(false, false);
    let symbols = spec.symbols.as_mut().unwrap();
    for name in ["$t", "$d", "$a.12", "$x"] {
        symbols.push(Named {
            kind: raw::STT_NOTYPE,
            bind: raw::STB_LOCAL,
            ..func(name, 0x0800_0100, 0, 0)
        });
    }
    symbols.push(func("$tick_global", 0x0800_0200, 4, 0));
    let elf = parse(&build(&spec));
    assert!(named(&elf, "$t").is_none());
    assert!(named(&elf, "$a.12").is_none());
    assert!(named(&elf, "$tick_global").is_some());
}

#[test]
fn an_address_finds_the_symbol_that_holds_it_or_the_nearest_below() {
    let (elf, lines) = read(&build(&rh850(0, &[(0x400, 0x40, 12)]))).unwrap();
    let (main, _) = named(&elf, "main").unwrap();
    assert_eq!(elf.symbol_at(0x410), Some(main));
    assert_eq!(elf.symbol_at(0x460), Some(main), "nearest below");
    let (tick, _) = named(&elf, "tick").unwrap();
    assert_eq!(elf.symbol_at(0x490), Some(tick));
    assert_eq!(lines.at(0x410), Some(("/work/src/main.c", 12)));
}
