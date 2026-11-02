use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::model::{CaseRule, RelPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleSource {
    Gitignore(RelPath),
    Exclude,
    User,
}

impl fmt::Display for RuleSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gitignore(folder) => {
                let folder = folder.as_path().display();
                if folder.is_empty() {
                    f.write_str(".gitignore")
                } else {
                    write!(f, "{folder}/.gitignore")
                }
            }
            Self::Exclude => f.write_str(".git/info/exclude"),
            Self::User => f.write_str("user ignore list"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleProblem {
    pub source: RuleSource,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    pub source: RuleSource,
    pub message: String,
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source, self.message)
    }
}

impl std::error::Error for RuleError {}

#[derive(Debug, Clone)]
pub enum IgnoreSource {
    Gitignore { folder: RelPath, contents: String },
    Exclude { contents: String },
}

#[derive(Debug, Clone)]
pub struct IgnoreRules {
    levels: HashMap<Vec<String>, Gitignore>,
    user: Option<Gitignore>,
    show_ignored: bool,
    case: CaseRule,
}

struct FolderBuild {
    source: RuleSource,
    builder: GitignoreBuilder,
}

struct RuleBuilder<'a> {
    case: CaseRule,
    problems: &'a mut Vec<RuleProblem>,
    folders: HashMap<Vec<String>, FolderBuild>,
}

impl IgnoreRules {
    pub fn new(
        sources: impl IntoIterator<Item = IgnoreSource>,
        user: &[String],
        show_ignored: bool,
        case: CaseRule,
    ) -> Result<(Self, Vec<RuleProblem>), RuleError> {
        let sources: Vec<IgnoreSource> = sources.into_iter().collect();
        let mut problems = Vec::new();
        let mut build = RuleBuilder::new(case, &mut problems);
        for source in &sources {
            if let IgnoreSource::Exclude { contents } = source {
                build.exclude(contents)?;
            }
        }
        for source in &sources {
            if let IgnoreSource::Gitignore { folder, contents } = source {
                build.gitignore(folder, contents)?;
            }
        }
        let levels = build.finish()?;
        let user = build_user(user, case, &mut problems)?;
        Ok((
            Self {
                levels,
                user,
                show_ignored,
                case,
            },
            problems,
        ))
    }

    #[must_use]
    pub fn is_ignored(&self, rel: &RelPath, is_dir: bool) -> bool {
        self.ignored(&segments(rel), is_dir)
    }

    #[must_use]
    pub fn keeps(&self, rel: &RelPath, is_dir: bool) -> bool {
        let path = segments(rel);
        !has_git_segment(&path, self.case) && (self.show_ignored || !self.ignored(&path, is_dir))
    }

    #[must_use]
    pub fn enters(&self, folder: &RelPath) -> bool {
        let path = segments(folder);
        !has_git_segment(&path, self.case) && !self.ignored(&path, true)
    }

    fn ignored(&self, path: &[String], is_dir: bool) -> bool {
        if has_git_segment(path, self.case) {
            return true;
        }
        let normalized = folder_key(path, self.case);
        for end in 1..path.len() {
            if self.matches(&path[..end], &normalized[..end], true) {
                return true;
            }
        }
        self.matches(path, &normalized, is_dir)
    }

    fn matches(&self, path: &[String], normalized: &[String], is_dir: bool) -> bool {
        let mut ignored = false;
        for depth in 0..path.len() {
            if let Some(matcher) = self.levels.get(&normalized[..depth]) {
                let relative = matcher_path(&path[depth..]);
                apply(&mut ignored, matcher.matched(relative, is_dir));
            }
        }
        if let Some(user) = &self.user {
            apply(&mut ignored, user.matched(matcher_path(path), is_dir));
        }
        ignored
    }
}

impl<'a> RuleBuilder<'a> {
    fn new(case: CaseRule, problems: &'a mut Vec<RuleProblem>) -> Self {
        Self {
            case,
            problems,
            folders: HashMap::new(),
        }
    }

    fn exclude(&mut self, contents: &str) -> Result<(), RuleError> {
        self.add(Vec::new(), RuleSource::Exclude, contents)
    }

    fn gitignore(&mut self, folder: &RelPath, contents: &str) -> Result<(), RuleError> {
        let key = folder_key(&segments(folder), self.case);
        self.add(key, RuleSource::Gitignore(folder.clone()), contents)
    }

    fn add(
        &mut self,
        key: Vec<String>,
        source: RuleSource,
        contents: &str,
    ) -> Result<(), RuleError> {
        let current = source.clone();
        let entry = match self.folders.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let builder = builder_for(self.case, source.clone())?;
                entry.insert(FolderBuild { source, builder })
            }
        };
        add_lines(&mut entry.builder, &current, contents, self.problems);
        Ok(())
    }

    fn finish(self) -> Result<HashMap<Vec<String>, Gitignore>, RuleError> {
        let mut levels = HashMap::with_capacity(self.folders.len());
        for (key, folder) in self.folders {
            match folder.builder.build() {
                Ok(matcher) => {
                    levels.insert(key, matcher);
                }
                Err(error) => {
                    return Err(RuleError {
                        source: folder.source,
                        message: error.to_string(),
                    });
                }
            }
        }
        Ok(levels)
    }
}

fn builder_for(case: CaseRule, source: RuleSource) -> Result<GitignoreBuilder, RuleError> {
    let mut builder = GitignoreBuilder::new(".");
    builder
        .case_insensitive(case == CaseRule::Insensitive)
        .map_err(|error| RuleError {
            source,
            message: error.to_string(),
        })?;
    Ok(builder)
}

fn add_lines(
    builder: &mut GitignoreBuilder,
    source: &RuleSource,
    contents: &str,
    problems: &mut Vec<RuleProblem>,
) {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    for (index, line) in contents.lines().enumerate() {
        if let Err(error) = builder.add_line(None, line) {
            problems.push(RuleProblem {
                source: source.clone(),
                line: index + 1,
                message: error.to_string(),
            });
        }
    }
}

fn build_user(
    user: &[String],
    case: CaseRule,
    problems: &mut Vec<RuleProblem>,
) -> Result<Option<Gitignore>, RuleError> {
    if user.is_empty() {
        return Ok(None);
    }
    let source = RuleSource::User;
    let mut builder = builder_for(case, source.clone())?;
    for (index, line) in user.iter().enumerate() {
        if let Err(error) = builder.add_line(None, line) {
            problems.push(RuleProblem {
                source: source.clone(),
                line: index + 1,
                message: error.to_string(),
            });
        }
    }
    match builder.build() {
        Ok(matcher) => Ok(Some(matcher)),
        Err(error) => Err(RuleError {
            source,
            message: error.to_string(),
        }),
    }
}

fn apply<T>(ignored: &mut bool, matched: Match<T>) {
    match matched {
        Match::Ignore(_) => *ignored = true,
        Match::Whitelist(_) => *ignored = false,
        Match::None => {}
    }
}

fn folder_key(path: &[String], case: CaseRule) -> Vec<String> {
    path.iter().map(|segment| fold(segment, case)).collect()
}

fn fold(segment: &str, case: CaseRule) -> String {
    match case {
        CaseRule::Sensitive => segment.to_owned(),
        CaseRule::Insensitive => segment.to_ascii_lowercase(),
    }
}

fn segments(rel: &RelPath) -> Vec<String> {
    rel.as_path()
        .to_text()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

fn matcher_path(path: &[String]) -> String {
    let mut joined = String::new();
    for (index, segment) in path.iter().enumerate() {
        if index > 0 {
            joined.push('/');
        }
        for character in segment.chars() {
            joined.push(if character == '\\' {
                '\u{f05c}'
            } else {
                character
            });
        }
    }
    joined
}

fn has_git_segment(path: &[String], case: CaseRule) -> bool {
    path.iter().any(|segment| match case {
        CaseRule::Sensitive => segment == ".git",
        CaseRule::Insensitive => segment.eq_ignore_ascii_case(".git"),
    })
}

#[cfg(test)]
mod tests;
