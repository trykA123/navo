#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    text: String,
    items: Vec<Vec<char>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("pattern has no items")]
    Empty,
}

const EVERY_NAME: &[char] = &['*', '.', '*'];

impl Pattern {
    pub fn parse(text: &str) -> Result<Self, PatternError> {
        let items: Vec<Vec<char>> = text
            .split(';')
            .map(|item| item.trim_matches(' '))
            .filter(|item| !item.is_empty())
            .map(fold)
            .collect();
        if items.is_empty() {
            return Err(PatternError::Empty);
        }
        Ok(Self {
            text: text.to_owned(),
            items,
        })
    }

    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        let name = fold(name);
        self.items
            .iter()
            .any(|item| item.as_slice() == EVERY_NAME || wildcard(item, &name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

fn fold(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

fn wildcard(pattern: &[char], name: &[char]) -> bool {
    let (mut p, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, n));
                p += 1;
            }
            Some(&c) if c == '?' || c == name[n] => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((sp, sn)) => {
                    p = sp + 1;
                    n = sn + 1;
                    star = Some((sp, sn + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}
