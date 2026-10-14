use std::collections::BTreeMap;

use serde::Deserialize;

use super::{CoverageError, FloorMiss};

#[derive(Deserialize)]
struct Floors {
    floors: BTreeMap<String, f64>,
}

pub fn parse_floors(toml: &str) -> Result<BTreeMap<String, f64>, CoverageError> {
    toml::from_str::<Floors>(toml)
        .map(|parsed| parsed.floors)
        .map_err(|error| CoverageError::InvalidFloors(error.to_string()))
}

pub fn misses(coverage: &BTreeMap<String, f64>, floors: &BTreeMap<String, f64>) -> Vec<FloorMiss> {
    floors
        .iter()
        .filter_map(|(crate_name, &floor)| {
            let actual = coverage.get(crate_name).copied();
            actual.is_none_or(|value| value < floor).then(|| FloorMiss {
                crate_name: crate_name.clone(),
                actual,
                floor,
            })
        })
        .collect()
}
