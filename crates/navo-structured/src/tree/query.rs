//! Parses a small JSONPath-like query and selects matching tree nodes.
//! It backs querying in the structured tree viewer and never loads lazy nodes.

use std::collections::HashSet;

use crate::{Key, NodeId, ParseError, Tree};

const STARTS_WITH_DOLLAR: &str = "a query starts with $";
const EXPECTED_NAME: &str = "expected a name after .";
const EXPECTED_INDEX: &str = "expected [n] or [*]";
const UNEXPECTED: &str = "unexpected character";

/// One parsed query step; it moves from a set of nodes to the next set.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    /// A direct child whose key is this name.
    Child(String),
    /// Every loaded child.
    AnyChild,
    /// The loaded child at this 0-based index.
    Nth(u32),
    /// A descendant at any depth with this name.
    Descendant(String),
    /// Every descendant at any depth.
    AnyDescendant,
}

/// A parsed query: steps applied in order from the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    steps: Vec<Step>,
}

impl Query {
    /// Parses a query; errors carry line 1 and the 1-based character column of the problem.
    pub fn parse(text: &str) -> Result<Query, ParseError> {
        let chars: Vec<char> = text.chars().collect();
        if chars.first() != Some(&'$') {
            return Err(ParseError::new(STARTS_WITH_DOLLAR, 1, 1));
        }
        let mut steps = Vec::new();
        let mut at = 1;
        while let Some(&ch) = chars.get(at) {
            let (step, next) = match ch {
                '.' => parse_dot(&chars, at)?,
                '[' => parse_bracket(&chars, at)?,
                _ => return Err(ParseError::new(UNEXPECTED, 1, column(at))),
            };
            steps.push(step);
            at = next;
        }
        Ok(Query { steps })
    }
}

/// Parses `.name`, `.*`, `..name` or `..*` at `at`; returns the step and the next index.
fn parse_dot(chars: &[char], at: usize) -> Result<(Step, usize), ParseError> {
    let descendant = chars.get(at + 1) == Some(&'.');
    let after = at + if descendant { 2 } else { 1 };
    match chars.get(after) {
        Some('*') => {
            let step = if descendant {
                Step::AnyDescendant
            } else {
                Step::AnyChild
            };
            Ok((step, after + 1))
        }
        Some(&ch) if is_name(ch) => {
            let mut end = after;
            while chars.get(end).is_some_and(|ch| is_name(*ch)) {
                end += 1;
            }
            let name = chars[after..end].iter().collect();
            let step = if descendant {
                Step::Descendant(name)
            } else {
                Step::Child(name)
            };
            Ok((step, end))
        }
        _ => Err(ParseError::new(EXPECTED_NAME, 1, column(after))),
    }
}

/// Parses `[n]` or `[*]` at `at`; returns the step and the next index.
fn parse_bracket(chars: &[char], at: usize) -> Result<(Step, usize), ParseError> {
    match chars.get(at + 1) {
        Some('*') if chars.get(at + 2) == Some(&']') => Ok((Step::AnyChild, at + 3)),
        Some(&ch) if ch.is_ascii_digit() => {
            let mut end = at + 1;
            let mut index = 0u32;
            while let Some(digit) = chars.get(end).and_then(|ch| ch.to_digit(10)) {
                index = index
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(digit))
                    .ok_or_else(|| index_error(at))?;
                end += 1;
            }
            if chars.get(end) != Some(&']') {
                return Err(index_error(at));
            }
            Ok((Step::Nth(index), end + 1))
        }
        _ => Err(index_error(at)),
    }
}

/// Builds the bracket error at the 1-based column of the opening bracket.
fn index_error(at: usize) -> ParseError {
    ParseError::new(EXPECTED_INDEX, 1, column(at))
}

/// Converts a 0-based character index into a 1-based column.
fn column(at: usize) -> u64 {
    at as u64 + 1
}

/// Reports whether `ch` may appear inside a `.name` step.
fn is_name(ch: char) -> bool {
    !matches!(ch, '.' | '[' | ']' | '*') && !ch.is_whitespace()
}

impl Tree {
    /// Returns node ids matching `query` in document order; the result has no duplicates.
    pub fn select(&self, query: &Query) -> Vec<NodeId> {
        self.select_with_count(query).0
    }

    /// Returns matching ids and how many ids the steps emitted, for tests.
    #[cfg(test)]
    pub(super) fn select_counted(&self, query: &Query) -> (Vec<NodeId>, usize) {
        self.select_with_count(query)
    }

    /// Applies the steps, dropping repeated ids per step; returns ids and emit count.
    fn select_with_count(&self, query: &Query) -> (Vec<NodeId>, usize) {
        let Some(root) = self.root() else {
            return (Vec::new(), 0);
        };
        let mut current = vec![root];
        let mut visited = 0;
        for step in &query.steps {
            let (next, count) = self.apply_step(&current, step);
            current = next;
            visited += count;
        }
        let matched: HashSet<NodeId> = current.into_iter().collect();
        let ordered = self
            .preorder()
            .into_iter()
            .filter(|id| matched.contains(id))
            .collect();
        (ordered, visited)
    }

    /// Applies one step to `current`, dropping repeated ids; returns next ids and emit count.
    fn apply_step(&self, current: &[NodeId], step: &Step) -> (Vec<NodeId>, usize) {
        let descendant = matches!(step, Step::Descendant(_) | Step::AnyDescendant);
        let mut seen = HashSet::new();
        let mut next = Vec::new();
        let mut visited = 0;
        for &id in current {
            if descendant && seen.contains(&id) {
                continue; // An earlier start already walked this subtree.
            }
            let (found, count) = self.collect_step(id, step, &mut seen);
            visited += count;
            next.extend(found);
        }
        (next, visited)
    }

    /// Returns the ids one step finds below `id` once, and how many it emitted.
    fn collect_step(
        &self,
        id: NodeId,
        step: &Step,
        seen: &mut HashSet<NodeId>,
    ) -> (Vec<NodeId>, usize) {
        let mut out = Vec::new();
        let mut visited = 0;
        for found in self.matches(id, step) {
            visited += 1;
            if seen.insert(found) {
                out.push(found);
            }
        }
        (out, visited)
    }

    /// Returns the direct matches of one step below `id`, in document order.
    fn matches(&self, id: NodeId, step: &Step) -> Vec<NodeId> {
        match step {
            Step::Child(name) => self
                .children(id)
                .filter(|child| self.has_name(*child, name))
                .collect(),
            Step::AnyChild => self.children(id).collect(),
            Step::Nth(index) => self.children(id).nth(*index as usize).into_iter().collect(),
            Step::Descendant(name) => self
                .descendants(id)
                .into_iter()
                .filter(|child| self.has_name(*child, name))
                .collect(),
            Step::AnyDescendant => self.descendants(id),
        }
    }

    /// Reports whether `id` has the key `Key::Name(name)`.
    fn has_name(&self, id: NodeId, name: &str) -> bool {
        matches!(self.key(id), Key::Name(found) if found == name)
    }

    /// Returns every strict descendant of `id` in pre-order.
    fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(id).collect();
        stack.reverse();
        while let Some(at) = stack.pop() {
            out.push(at);
            let mut children: Vec<NodeId> = self.children(at).collect();
            children.reverse();
            stack.extend(children);
        }
        out
    }

    /// Returns all node ids from the root in pre-order.
    fn preorder(&self) -> Vec<NodeId> {
        let Some(root) = self.root() else {
            return Vec::new();
        };
        let mut out = vec![root];
        out.extend(self.descendants(root));
        out
    }
}
