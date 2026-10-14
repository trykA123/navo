use std::{fmt, path::PathBuf};

use serde::Deserialize;

use super::{CEILING, Exception, SizeError};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExceptionList {
    #[serde(default)]
    exception: Vec<ExceptionEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExceptionEntry {
    path: PathBuf,
    limit: usize,
    #[serde(default)]
    reason: String,
}

pub fn parse_exceptions(source: &str) -> Result<Vec<Exception>, SizeError> {
    let parsed: ExceptionList =
        toml::from_str(source).map_err(|error| SizeError::InvalidToml(error.to_string()))?;
    parsed
        .exception
        .into_iter()
        .map(validate_exception)
        .collect()
}

fn validate_exception(entry: ExceptionEntry) -> Result<Exception, SizeError> {
    if entry.limit > CEILING {
        return Err(SizeError::LimitAboveCeiling {
            path: entry.path,
            limit: entry.limit,
        });
    }
    if entry.reason.trim().is_empty() {
        return Err(SizeError::MissingReason { path: entry.path });
    }
    Ok(Exception {
        path: entry.path,
        limit: entry.limit,
        reason: entry.reason,
    })
}

impl fmt::Display for SizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToml(message) => write!(formatter, "invalid size exceptions: {message}"),
            Self::LimitAboveCeiling { path, limit } => write!(
                formatter,
                "{}: exception limit {limit} exceeds ceiling {CEILING}",
                path.display()
            ),
            Self::MissingReason { path } => write!(
                formatter,
                "{}: exception requires a non-blank reason",
                path.display()
            ),
        }
    }
}

impl std::error::Error for SizeError {}
