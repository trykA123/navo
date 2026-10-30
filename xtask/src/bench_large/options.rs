use std::path::PathBuf;

pub(super) struct Options {
    pub quick: bool,
    pub huge: bool,
    pub json: Option<PathBuf>,
}

pub(super) fn parse(args: &[String]) -> Result<Options, String> {
    let mut quick = false;
    let mut huge = false;
    let mut json = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--quick" if !quick => quick = true,
            "--huge" if !huge => huge = true,
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
    if quick && huge {
        return Err("--quick and --huge cannot be combined".to_owned());
    }
    Ok(Options { quick, huge, json })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_the_three_flags() {
        let args: Vec<String> = ["--quick", "--json", "out.json"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        let options = parse(&args).unwrap();
        assert!(options.quick);
        assert_eq!(options.json, Some(PathBuf::from("out.json")));
    }

    #[test]
    fn parse_rejects_quick_with_huge() {
        let args: Vec<String> = ["--quick", "--huge"]
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        assert!(parse(&args).is_err());
    }
}
