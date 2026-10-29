use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    List,
    Size,
    Walk,
    WalkDua,
    WalkIgnore,
    Search,
    Save,
    Load,
}

impl Target {
    pub fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Size => "size",
            Self::Walk => "walk",
            Self::WalkDua => "walk-dua",
            Self::WalkIgnore => "walk-ignore",
            Self::Search => "search",
            Self::Save => "save",
            Self::Load => "load",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Options {
    pub root: PathBuf,
    pub runs: usize,
    pub warmup: usize,
    pub prepare: Option<String>,
    pub targets: Vec<Target>,
}

pub(super) fn parse(args: &[String]) -> Result<Options, String> {
    let Some((root, rest)) = args.split_first() else {
        return Err("a fixture folder is required".to_owned());
    };
    if root.is_empty() || root.starts_with("--") {
        return Err("a fixture folder is required before options".to_owned());
    }
    let (mut runs, mut warmup, mut prepare, mut targets) = (None, None, None, None);
    let (pairs, remainder) = rest.as_chunks::<2>();
    for pair in pairs {
        match pair[0].as_str() {
            "--runs" if runs.is_none() => runs = Some(number(&pair[1], "--runs", 1)?),
            "--warmup" if warmup.is_none() => warmup = Some(number(&pair[1], "--warmup", 0)?),
            "--prepare" if prepare.is_none() && !pair[1].trim().is_empty() => {
                prepare = Some(pair[1].clone());
            }
            "--targets" if targets.is_none() => targets = Some(parse_targets(&pair[1])?),
            option => return Err(format!("unknown, repeated or invalid option: {option}")),
        }
    }
    if !remainder.is_empty() {
        return Err("each option requires a value".to_owned());
    }
    Ok(Options {
        root: PathBuf::from(root),
        runs: runs.unwrap_or(10),
        warmup: warmup.unwrap_or(2),
        prepare,
        targets: targets.unwrap_or_else(|| vec![Target::List, Target::Size]),
    })
}

fn number(value: &str, flag: &str, minimum: usize) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value >= minimum)
        .ok_or_else(|| format!("{flag} must be an integer >= {minimum}"))
}

fn parse_targets(value: &str) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    let value = if value == "index" {
        "walk,search,save,load"
    } else {
        value
    };
    for name in value.split(',') {
        let target = match name {
            "list" => Target::List,
            "size" => Target::Size,
            "walk" => Target::Walk,
            "walk-dua" => Target::WalkDua,
            "walk-ignore" => Target::WalkIgnore,
            "search" => Target::Search,
            "save" => Target::Save,
            "load" => Target::Load,
            _ => return Err(format!("unknown target: {name}")),
        };
        if targets.contains(&target) {
            return Err(format!("repeated target: {name}"));
        }
        targets.push(target);
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_values(values: &[&str]) -> Result<Options, String> {
        parse(
            &values
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn defaults_to_ten_runs_two_warmups_and_both_targets() {
        assert_eq!(
            parse_values(&["fixture"]).unwrap(),
            Options {
                root: PathBuf::from("fixture"),
                runs: 10,
                warmup: 2,
                prepare: None,
                targets: vec![Target::List, Target::Size],
            }
        );
    }

    #[test]
    fn parses_each_flag_and_preserves_target_order() {
        let options = parse_values(&[
            "fixture",
            "--targets",
            "size,list",
            "--warmup",
            "0",
            "--prepare",
            "echo ready",
            "--runs",
            "3",
        ])
        .unwrap();
        assert_eq!(options.runs, 3);
        assert_eq!(options.warmup, 0);
        assert_eq!(options.prepare.as_deref(), Some("echo ready"));
        assert_eq!(options.targets, vec![Target::Size, Target::List]);
        assert_eq!(
            parse_values(&["fixture", "--targets", "list"])
                .unwrap()
                .targets,
            vec![Target::List]
        );
    }

    #[test]
    fn accepts_index_targets_and_shorthand() {
        assert_eq!(
            parse_values(&["fixture", "--targets", "walk,walk-dua,walk-ignore"])
                .unwrap()
                .targets,
            vec![Target::Walk, Target::WalkDua, Target::WalkIgnore]
        );
        let expected = vec![Target::Walk, Target::Search, Target::Save, Target::Load];
        for value in ["walk,search,save,load", "index"] {
            assert_eq!(
                parse_values(&["fixture", "--targets", value])
                    .unwrap()
                    .targets,
                expected
            );
        }
        assert_eq!(
            parse_values(&["fixture", "--targets", "load,list,save,size,search,walk"])
                .unwrap()
                .targets,
            vec![
                Target::Load,
                Target::List,
                Target::Save,
                Target::Size,
                Target::Search,
                Target::Walk
            ]
        );
    }

    #[test]
    fn rejects_missing_bad_unknown_and_repeated_arguments() {
        for values in [
            vec![],
            vec![""],
            vec!["--runs", "1"],
            vec!["fixture", "extra"],
            vec!["fixture", "--runs"],
            vec!["fixture", "--runs", "0"],
            vec!["fixture", "--runs", "-1"],
            vec!["fixture", "--runs", "x"],
            vec!["fixture", "--runs", "184467440737095516160"],
            vec!["fixture", "--warmup", "-1"],
            vec!["fixture", "--warmup", "x"],
            vec!["fixture", "--prepare", " "],
            vec!["fixture", "--unknown", "1"],
            vec!["fixture", "--targets", "unknown"],
            vec!["fixture", "--targets", ""],
            vec!["fixture", "--targets", "list,"],
            vec!["fixture", "--targets", "list,list"],
        ] {
            assert!(parse_values(&values).is_err(), "{values:?}");
        }
        for (flag, value) in [
            ("--runs", "1"),
            ("--warmup", "1"),
            ("--prepare", "true"),
            ("--targets", "list"),
        ] {
            assert!(parse_values(&["fixture", flag, value, flag, value]).is_err());
        }
    }
}
