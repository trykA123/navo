use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Options {
    pub quick: bool,
    pub json: Option<PathBuf>,
}

pub(super) fn parse(args: &[String]) -> Result<Options, String> {
    let mut quick = false;
    let mut json = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--quick" if !quick => quick = true,
            "--json" if json.is_none() => {
                let value = iter.next().ok_or("--json requires a path")?;
                if value.is_empty() || value.starts_with("--") {
                    return Err("--json requires a path".to_owned());
                }
                json = Some(PathBuf::from(value));
            }
            option => return Err(format!("unknown or repeated option: {option}")),
        }
    }
    Ok(Options { quick, json })
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
    fn parses_quick_and_json() {
        assert_eq!(
            parse_values(&[]).unwrap(),
            Options {
                quick: false,
                json: None
            }
        );
        assert_eq!(
            parse_values(&["--quick", "--json", "out.json"]).unwrap(),
            Options {
                quick: true,
                json: Some(PathBuf::from("out.json"))
            }
        );
        assert_eq!(
            parse_values(&["--json", "a", "--quick"]).unwrap().json,
            Some(PathBuf::from("a"))
        );
    }

    #[test]
    fn rejects_unknown_missing_and_repeated_options() {
        for values in [
            vec!["--unknown"],
            vec!["--json"],
            vec!["--json", "--quick"],
            vec!["--quick", "--quick"],
            vec!["--json", "a", "--json", "b"],
        ] {
            assert!(parse_values(&values).is_err(), "{values:?}");
        }
    }
}
