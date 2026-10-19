use std::path::PathBuf;

use super::fixture::{DATA_SETS, DataSet};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Options {
    pub target: PathBuf,
    pub runs: usize,
    pub sets: Vec<DataSet>,
}

pub(super) fn parse(args: &[String]) -> Result<Options, String> {
    let Some((target, rest)) = args.split_first() else {
        return Err("a target folder is required".to_owned());
    };
    if target.is_empty() || target.starts_with("--") {
        return Err("a target folder is required before options".to_owned());
    }
    let mut runs = None;
    let mut sets = None;
    let (pairs, remainder) = rest.as_chunks::<2>();
    for pair in pairs {
        match pair[0].as_str() {
            "--runs" if runs.is_none() => {
                runs = Some(parse_runs(&pair[1])?);
            }
            "--sets" if sets.is_none() => {
                sets = Some(parse_sets(&pair[1])?);
            }
            option => return Err(format!("unknown or repeated option: {option}")),
        }
    }
    if !remainder.is_empty() {
        return Err("each option requires a value".to_owned());
    }
    Ok(Options {
        target: PathBuf::from(target),
        runs: runs.unwrap_or(3),
        sets: sets.unwrap_or_else(|| DATA_SETS.to_vec()),
    })
}

fn parse_runs(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|runs| *runs > 0)
        .ok_or_else(|| "--runs must be a positive integer".to_owned())
}

fn parse_sets(value: &str) -> Result<Vec<DataSet>, String> {
    let mut sets = Vec::new();
    for name in value.split(',') {
        let set = DATA_SETS
            .iter()
            .find(|set| set.name == name)
            .ok_or_else(|| format!("unknown data set: {name}"))?;
        if sets.contains(set) {
            return Err(format!("repeated data set: {name}"));
        }
        sets.push(*set);
    }
    Ok(sets)
}
