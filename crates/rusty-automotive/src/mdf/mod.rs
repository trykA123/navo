mod parse;

pub use parse::{parse_mdf, sniff_mdf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    pub name: String,
    pub id: u64,
    pub mau: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub name: String,
    pub id: u64,
    pub mau: u64,
    pub size: u64,
    pub kind: ChipKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Rom,
    Ram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapLine {
    pub space: u64,
    pub start: u64,
    pub size: u64,
    pub chip: u64,
    pub chip_start: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdfProblem {
    Malformed { line: u64, reason: &'static str },
    CapReached { line: u64, limit: &'static str },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mdf {
    pub spaces: Vec<Space>,
    pub chips: Vec<Chip>,
    pub maps: Vec<MapLine>,
    pub problems: Vec<MdfProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub start: Option<u64>,
    pub start_estimated: bool,
    pub size: u64,
    pub kind: Option<ChipKind>,
    pub reserved: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TableOrigin {
    #[default]
    Mdf,
    Map,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegionTable {
    pub regions: Vec<Region>,
    pub origin: TableOrigin,
}

impl Mdf {
    #[must_use]
    pub fn regions(&self) -> RegionTable {
        let mut regions: Vec<Region> = self
            .maps
            .iter()
            .filter_map(|map| {
                let chip = self.mapped_chip(map).ok()?;
                Some(Region {
                    name: chip.name.clone(),
                    start: Some(map.start),
                    start_estimated: false,
                    size: map.size,
                    kind: Some(chip.kind),
                    reserved: None,
                })
            })
            .collect();
        for at in 1..regions.len() {
            let base = regions[at].name.clone();
            let mut copy = 2;
            while regions[..at]
                .iter()
                .any(|other| other.name.eq_ignore_ascii_case(&regions[at].name))
            {
                regions[at].name = format!("{base} ({copy})");
                copy += 1;
            }
        }
        RegionTable {
            regions,
            origin: TableOrigin::Mdf,
        }
    }

    fn mapped_chip(&self, map: &MapLine) -> Result<&Chip, &'static str> {
        if !self.spaces.iter().any(|space| space.id == map.space) {
            return Err("unknown space id");
        }
        let chip = self
            .chips
            .iter()
            .find(|chip| chip.id == map.chip)
            .ok_or("unknown chip id")?;
        if map.start.checked_add(map.size).is_none()
            || map.chip_start.checked_add(map.size).is_none()
        {
            return Err("mapping range overflows");
        }
        Ok(chip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE_CHIPS: &str = "# invented memory\n\
        space code 1 8\nspace data 2 8\n\
        chip pflash0 10 8 256 rom\nchip pflash1 11 8 128 rom\nchip dspr0 20 8 64 ram\n\
        map 1 0x80000000 256 10 0\nmap 1 0x80000100 128 11 0\nmap 2 0x20000000 64 20 0\n";

    fn has_problem(model: &Mdf, line: u64, reason: &'static str) -> bool {
        model
            .problems
            .contains(&MdfProblem::Malformed { line, reason })
    }

    #[test]
    fn a_chip_mapped_twice_gets_numbered_region_names() {
        let model = parse_mdf(
            "space a 1 8\nspace b 2 8\nchip x 10 8 64 ram\n\
             map 1 0x100 64 10 0\nmap 2 0x100 64 10 0\nmap 2 0x200 64 10 0\n",
        );
        let names: Vec<_> = model
            .regions()
            .regions
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["x", "x (2)", "x (3)"]);
    }

    #[test]
    fn three_mapped_chips_become_regions() {
        let model = parse_mdf(THREE_CHIPS);
        assert!(model.problems.is_empty(), "{:?}", model.problems);
        assert_eq!(model.spaces.len(), 2);
        assert_eq!(model.chips.len(), 3);
        assert_eq!(model.maps.len(), 3);
        assert_eq!(
            model.regions().regions,
            vec![
                Region {
                    name: "pflash0".into(),
                    start: Some(0x8000_0000),
                    start_estimated: false,
                    size: 256,
                    kind: Some(ChipKind::Rom),
                    reserved: None,
                },
                Region {
                    name: "pflash1".into(),
                    start: Some(0x8000_0100),
                    start_estimated: false,
                    size: 128,
                    kind: Some(ChipKind::Rom),
                    reserved: None,
                },
                Region {
                    name: "dspr0".into(),
                    start: Some(0x2000_0000),
                    start_estimated: false,
                    size: 64,
                    kind: Some(ChipKind::Ram),
                    reserved: None,
                },
            ]
        );
    }

    #[test]
    fn hex_decimal_tabs_and_comments_are_accepted() {
        let model = parse_mdf(
            "\r\n # header\r\nspace\tlocal\t0x1\t8 # space\r\nchip\tdspr0\t0X2\t0x8\t64\tram#chip\r\nmap\t1\t0x100\t16\t2\t0x20 # map\r\n",
        );
        assert!(model.problems.is_empty(), "{:?}", model.problems);
        assert_eq!(
            model.spaces,
            vec![Space {
                name: "local".into(),
                id: 1,
                mau: 8
            }]
        );
        assert_eq!(
            model.chips,
            vec![Chip {
                name: "dspr0".into(),
                id: 2,
                mau: 8,
                size: 64,
                kind: ChipKind::Ram
            }]
        );
        assert_eq!(
            model.maps,
            vec![MapLine {
                space: 1,
                start: 256,
                size: 16,
                chip: 2,
                chip_start: 32
            }]
        );
        let regions = model.regions();
        assert_eq!(
            (regions.regions[0].start, regions.regions[0].size),
            (Some(256), 16)
        );
    }

    #[test]
    fn unknown_chip_is_reported_without_losing_the_map() {
        let model = parse_mdf("space local 1 8\nmap 1 256 16 99 0\n");
        assert_eq!(model.maps.len(), 1);
        assert!(has_problem(&model, 2, "unknown chip id"));
        assert!(model.regions().regions.is_empty());
    }

    #[test]
    fn unknown_space_is_reported_without_losing_the_map() {
        let model = parse_mdf("chip dspr0 2 8 64 ram\nmap 99 256 16 2 0\n");
        assert_eq!(model.maps.len(), 1);
        assert!(has_problem(&model, 2, "unknown space id"));
        assert!(model.regions().regions.is_empty());
    }

    #[test]
    fn maps_can_precede_definitions() {
        let model = parse_mdf("map 1 256 16 2 0\nchip dspr0 2 8 64 ram\nspace local 1 8\n");
        assert!(model.problems.is_empty(), "{:?}", model.problems);
        assert_eq!(model.regions().regions.len(), 1);
    }

    #[test]
    fn overlapping_maps_are_retained_and_reported() {
        let model = parse_mdf(
            "space local 1 8\nchip dspr0 2 8 64 ram\nmap 1 256 32 2 0\nmap 1 272 16 2 32\n",
        );
        assert_eq!(model.maps.len(), 2);
        assert!(has_problem(&model, 4, "overlapping maps in address space"));
        assert_eq!(model.regions().regions.len(), 2);
    }

    #[test]
    fn nested_unsorted_maps_report_all_overlaps() {
        let model = parse_mdf(
            "space local 1 8\nchip dspr0 2 8 256 ram\nmap 1 304 16 2 0\nmap 1 256 128 2 0\nmap 1 272 16 2 0\n",
        );
        assert!(has_problem(&model, 3, "overlapping maps in address space"));
        assert!(has_problem(&model, 5, "overlapping maps in address space"));
        assert_eq!(model.regions().regions.len(), 3);
    }

    #[test]
    fn adjacent_maps_and_separate_spaces_do_not_overlap() {
        let model = parse_mdf(
            "space local 1 8\nspace global 3 8\nchip dspr0 2 8 64 ram\nmap 1 256 16 2 0\nmap 1 272 16 2 16\nmap 3 256 16 2 0\n",
        );
        assert!(model.problems.is_empty(), "{:?}", model.problems);
        assert_eq!(model.regions().regions.len(), 3);
    }

    #[test]
    fn malformed_lines_report_their_line_and_parsing_continues() {
        for broken in [
            "space",
            "chip dspr0 2 8 64",
            "map 1 0 16 2",
            "unknown 1",
            "chip dspr0 2 8 64 other",
            "chip dspr0 2 8 bad ram",
            "map 1 0 nope 2 0",
            "space local -1 8",
            "space local 0xGG 8",
            "space local 0x 8",
            "space local 18446744073709551616 8",
        ] {
            let model = parse_mdf(&format!("# header\n{broken}\nspace kept 7 8\n"));
            assert_eq!(model.spaces.len(), 1, "{broken}");
            assert_eq!(model.spaces[0].name, "kept", "{broken}");
            assert!(
                matches!(
                    model.problems.as_slice(),
                    [MdfProblem::Malformed { line: 2, .. }]
                ),
                "{broken}: {:?}",
                model.problems
            );
        }
    }

    #[test]
    fn trailing_fields_are_tolerated_with_a_problem() {
        let model = parse_mdf(
            "space local 1 8 extra\nchip dspr0 2 8 64 ram extra\nmap 1 256 16 2 0 extra\n",
        );
        assert_eq!(model.regions().regions.len(), 1);
        for line in 1..=3 {
            assert!(has_problem(&model, line, "extra fields ignored"));
        }
    }

    #[test]
    fn duplicate_ids_keep_the_first_definition() {
        let model = parse_mdf(
            "space local 1 8\nspace other 1 8\nchip dspr0 2 8 64 ram\nchip pflash0 2 8 64 rom\n",
        );
        assert_eq!(model.spaces.len(), 1);
        assert_eq!(model.chips.len(), 1);
        assert!(has_problem(&model, 2, "duplicate space id"));
        assert!(has_problem(&model, 4, "duplicate chip id"));
    }

    #[test]
    fn overflowing_map_ranges_are_reported_and_not_regions() {
        for map in [
            "map 1 0xffffffffffffffff 2 2 0",
            "map 1 0 2 2 0xffffffffffffffff",
        ] {
            let model = parse_mdf(&format!("space local 1 8\nchip dspr0 2 8 64 ram\n{map}\n"));
            assert_eq!(model.maps.len(), 1);
            assert!(has_problem(&model, 3, "mapping range overflows"));
            assert!(model.regions().regions.is_empty());
        }
    }

    #[test]
    fn sniff_accepts_each_keyword_after_comments() {
        for line in ["space local 1 8", "chip dspr0 2 8 64 ram", "map 1 0 16 2 0"] {
            assert!(sniff_mdf(format!("\r\n # header\r\n\t{line}\n").as_bytes()));
        }
    }

    #[test]
    fn sniff_rejects_mdf4_and_unrelated_heads() {
        for head in [
            b"MDF     4.10\0space local 1 8".as_slice(),
            b"MDF     \nspace local 1 8",
            b"spaces local 1 8",
            b"chipset dspr0",
            b"# only comments\n",
            b"text\nspace local 1 8",
            b"\xff\x00space local 1 8",
            b"",
        ] {
            assert!(!sniff_mdf(head), "{head:?}");
        }
    }

    #[test]
    fn ten_thousand_lines_are_accepted() {
        let model = parse_mdf(&format!("{}space local 1 8\n", "# comment\n".repeat(9_999)));
        assert_eq!(model.spaces.len(), 1);
        assert!(model.problems.is_empty());
    }

    #[test]
    fn line_ten_thousand_and_one_stops_with_a_problem() {
        let model = parse_mdf(&format!(
            "{}space ignored 1 8\n",
            "# comment\n".repeat(10_000)
        ));
        assert!(model.spaces.is_empty());
        assert_eq!(
            model.problems,
            vec![MdfProblem::CapReached {
                line: 10_001,
                limit: "lines"
            }]
        );
    }
}
