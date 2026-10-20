use std::path::PathBuf;

use crate::pages::PAGES;

pub const USAGE: &str = "usage: rusty-ui-gallery [--screenshot <dir> [--pages <stem,stem,...>]]";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub screenshot_dir: Option<PathBuf>,
    pub pages: Option<Vec<String>>,
}

pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--screenshot" => {
                let dir = args.next().ok_or("--screenshot needs a directory")?;
                options.screenshot_dir = Some(PathBuf::from(dir));
            }
            "--pages" => {
                let list = args.next().ok_or("--pages needs a list of page stems")?;
                options.pages = Some(parse_pages(&list)?);
            }
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    if options.pages.is_some() && options.screenshot_dir.is_none() {
        return Err("--pages needs --screenshot".to_string());
    }
    Ok(options)
}

fn parse_pages(list: &str) -> Result<Vec<String>, String> {
    let stems: Vec<String> = list.split(',').map(str::to_string).collect();
    let unknown: Vec<&str> = stems
        .iter()
        .map(String::as_str)
        .filter(|stem| !PAGES.iter().any(|info| info.stem == *stem))
        .collect();
    if unknown.is_empty() {
        Ok(stems)
    } else {
        Err(format!("unknown page {}", unknown.join(", ")))
    }
}
