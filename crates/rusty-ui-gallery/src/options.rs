use std::path::PathBuf;

pub const USAGE: &str = "usage: rusty-ui-gallery [--screenshot <dir>]";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub screenshot_dir: Option<PathBuf>,
}

pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        if flag != "--screenshot" {
            return Err(format!("unknown argument {flag}"));
        }
        let dir = args.next().ok_or("--screenshot needs a directory")?;
        options.screenshot_dir = Some(PathBuf::from(dir));
    }
    Ok(options)
}
