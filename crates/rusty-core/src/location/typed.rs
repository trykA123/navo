use super::parent::root_len;
use super::{Location, ProviderPath, windows_style};

pub fn typed_location(
    input: &str,
    current: &Location,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Location, String> {
    let text = input.trim().trim_matches('"').trim();
    if text.is_empty() {
        return Err("Type a path.".to_owned());
    }
    let text = home(&expand(text, env)?, env)?;
    let windows = current.is_local() && windows_style(current.as_str());
    let path = if windows {
        windows_path(&text, current)?
    } else {
        slash_path(&text, current)
    };
    Location::with_provider(current.provider().clone(), ProviderPath::from_text(&path))
        .map_err(|error| error.to_string())
}

fn expand(text: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let Some(length) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + length];
        out.push_str(&rest[..start]);
        if name.is_empty() {
            out.push('%');
        } else {
            out.push_str(&env(name).ok_or_else(|| format!("Unknown variable %{name}%."))?);
        }
        rest = &rest[start + length + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn home(text: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let rest = if text == "~" {
        ""
    } else if let Some(rest) = text.strip_prefix("~\\").or_else(|| text.strip_prefix("~/")) {
        rest
    } else {
        return Ok(text.to_owned());
    };
    let base = env("USERPROFILE")
        .or_else(|| env("HOME"))
        .ok_or_else(|| "No home folder is set.".to_owned())?;
    Ok(if rest.is_empty() {
        base
    } else {
        format!("{}/{rest}", base.trim_end_matches(['\\', '/']))
    })
}

fn is_windows_absolute(text: &str) -> bool {
    text.starts_with("\\\\") || windows_style(text)
}

fn windows_path(text: &str, current: &Location) -> Result<String, String> {
    if text.starts_with('/') && !text.starts_with("//") {
        return Err(format!("Not a Windows path: {text}"));
    }
    let text = text.replace('/', "\\");
    let joined = if is_windows_absolute(&text) {
        text
    } else {
        format!("{}\\{text}", current.as_str())
    };
    let root_end = root_len(&joined, true);
    let mut root = joined[..root_end].to_owned();
    if !root.ends_with('\\') {
        root.push('\\');
    }
    Ok(format!("{root}{}", segments(&joined[root_end..], '\\')))
}

fn slash_path(text: &str, current: &Location) -> String {
    let base = current.path().to_text();
    let joined = if text.starts_with('/') {
        text.to_owned()
    } else {
        format!("{}/{text}", base.trim_end_matches('/'))
    };
    let root = if joined.starts_with('/') { "/" } else { "" };
    let rest = segments(&joined, '/');
    if rest.is_empty() && root.is_empty() {
        return base;
    }
    format!("{root}{rest}")
}

fn segments(rest: &str, separator: char) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for part in rest.split(['\\', '/']) {
        match part {
            "" | "." => {}
            ".." => {
                kept.pop();
            }
            name => kept.push(name),
        }
    }
    kept.join(&separator.to_string())
}
