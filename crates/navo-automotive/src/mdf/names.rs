use std::collections::{HashMap, HashSet};

use super::Region;

pub(super) fn number_copies(regions: &mut [Region]) {
    let mut taken: HashSet<String> = HashSet::with_capacity(regions.len());
    let mut next: HashMap<String, u64> = HashMap::new();
    for region in regions {
        let key = region.name.to_ascii_lowercase();
        if taken.insert(key.clone()) {
            continue;
        }
        let copy = next.entry(key).or_insert(2);
        loop {
            let name = format!("{} ({copy})", region.name);
            *copy += 1;
            if taken.insert(name.to_ascii_lowercase()) {
                region.name = name;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::{MapLine, parse_mdf};

    #[test]
    fn copies_skip_a_name_another_chip_already_has() {
        let mut model = parse_mdf("space a 1 8\nchip x 10 8 16 ram\nchip y 11 8 16 ram\n");
        model.chips[1].name = "X (2)".into();
        model.maps = [10, 11, 10, 10]
            .into_iter()
            .enumerate()
            .map(|(at, chip)| MapLine {
                space: 1,
                start: at as u64 * 16,
                size: 16,
                chip,
                chip_start: 0,
            })
            .collect();
        let names: Vec<_> = model
            .regions()
            .regions
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["x", "X (2)", "x (3)", "x (4)"]);
    }

    #[test]
    fn ten_thousand_copies_of_one_chip_are_numbered_quickly() {
        let mut model = parse_mdf("space a 1 8\nchip x 10 8 16 ram\n");
        model.maps = (0..10_000u64)
            .map(|at| MapLine {
                space: 1,
                start: at * 16,
                size: 16,
                chip: 10,
                chip_start: 0,
            })
            .collect();
        let started = std::time::Instant::now();
        let regions = model.regions().regions;
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(regions.len(), 10_000);
        assert_eq!(regions[9_999].name, "x (10000)");
        let unique: HashSet<String> = regions
            .iter()
            .map(|r| r.name.to_ascii_lowercase())
            .collect();
        assert_eq!(unique.len(), 10_000);
    }
}
