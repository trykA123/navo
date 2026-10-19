use std::sync::atomic::AtomicBool;

use rusty_automotive::map::{MapModel, index_map};

#[allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]
fn parse(body: &str) -> MapModel {
    let text = format!("TASKING VX\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

const OBJECTS: &str = "* Link Result
| File | Section | Size | Offset | Section | Size |
| orbit.o | .one | 4 | 0 | .code | 16 |
|| .two | 4 | 4 | .code | 16 |
|| .three | 2 | 0 | .data | 8 |
| lunar.o | .four | 3 | 0 | .code | 16 |
| comet.o | .unknown | 1 | 0 | .missing | 16 |
* Locate Result
| Section | Address | Size | Chip |
| .code | 0x100 | 16 | orbit_rom |
| .data | 0x200 | 8 | orbit_ram |
* Memory Usage
| Memory | Code | Data | Reserved | Free | Total |
| orbit_rom | 16 | 0 | 0 | 16 | 32 |
| orbit_ram | 0 | 8 | 0 | 8 | 16 |
";

#[test]
fn contributions_resolve_regions_and_object_totals_can_filter_them() {
    let model = parse(OBJECTS);
    assert_eq!(model.contributions[0].region.as_deref(), Some("orbit_rom"));
    assert_eq!(model.contributions[2].region.as_deref(), Some("orbit_ram"));
    assert_eq!(model.contributions[4].region, None);
    let totals = model.object_totals(Some("orbit_rom"));
    assert_eq!(totals.len(), 2);
    assert_eq!(
        (totals[0].object_file, totals[0].bytes, totals[0].region),
        ("lunar.o", 3, Some("orbit_rom"))
    );
    assert_eq!((totals[1].object_file, totals[1].bytes), ("orbit.o", 8));
    assert_eq!(model.object_totals(None).len(), 4);
    assert!(model.object_totals(Some("absent")).is_empty());
    let regions = model.region_totals();
    assert_eq!(regions.len(), 2);
    assert_eq!(
        (regions[0].region, regions[0].used, regions[0].free),
        ("orbit_ram", 8, 8)
    );
    assert_eq!((regions[1].used, regions[1].free), (16, 16));
}

#[test]
fn conflicting_region_or_space_leaves_contributions_unresolved() {
    for extra in [
        "| .code | 0x100 | 16 | lunar_rom | one |",
        "| .code | 0x100 | 16 | orbit_rom | two |",
    ] {
        let model = parse(&format!(
            "* Link Result\n| File | Section | Size | Offset | Section | Size |\n| orbit.o | .one | 4 | 0 | .code | 16 |\n* Locate Result\n| Section | Address | Size | Chip | Space |\n| .code | 0x100 | 16 | orbit_rom | one |\n{extra}\n"
        ));
        assert_eq!(model.contributions[0].region, None);
        assert_eq!(model.contributions[0].address, None);
    }
}

#[test]
fn address_index_searches_unsorted_symbols_with_space_and_size_boundaries() {
    let model = parse(
        "* Symbols (sorted on name)\n| Name | Address | Size | Space |\n| zeta | 0x10 | 4 | orbit |\n| alpha | 0x30 | - | orbit |\n| lunar | 0x10 | 8 | lunar |\n| zero | 0x20 | 0 | orbit |\n",
    );
    let index = model.symbol_index();
    assert!(index.symbol_at(0x0f, Some("orbit")).is_none());
    assert_eq!(index.symbol_at(0x10, Some("orbit")).unwrap().name, "zeta");
    assert_eq!(index.symbol_at(0x13, Some("orbit")).unwrap().name, "zeta");
    assert!(index.symbol_at(0x14, Some("orbit")).is_none());
    assert_eq!(index.symbol_at(0x17, Some("lunar")).unwrap().name, "lunar");
    assert_eq!(index.symbol_at(0x20, Some("orbit")).unwrap().name, "zero");
    assert!(index.symbol_at(0x21, Some("orbit")).is_none());
    assert_eq!(index.symbol_at(0x40, Some("orbit")).unwrap().name, "alpha");
    assert!(index.symbol_at(0x10, None).is_none());
    assert!(
        parse("* Link Result")
            .symbol_index()
            .symbol_at(0, None)
            .is_none()
    );
}
