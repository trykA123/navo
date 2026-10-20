use std::io::BufRead;

use super::convert::Block;
use super::{A2lModel, InterfaceData, Module};
use crate::Watch;
use crate::description::{DescriptionError, Lexer, Token, TokenKind};

const MAX_DEPTH: usize = 128;

struct Scope {
    kind: String,
    line: u64,
    module: Option<usize>,
    named: bool,
}

#[derive(Default)]
struct Scanner {
    model: A2lModel,
    scopes: Vec<Scope>,
    block: Option<Block>,
    stopped: bool,
}

impl Scanner {
    fn module(&self) -> Option<usize> {
        self.scopes.iter().rev().find_map(|scope| scope.module)
    }

    fn append(&mut self, text: &str, line: u64) {
        if self
            .scopes
            .iter()
            .any(|scope| matches!(scope.kind.as_str(), "IF_DATA" | "A2ML"))
        {
            return;
        }
        if let Some(block) = &mut self.block {
            block.push(text, line);
        }
    }

    fn begin(&mut self, tag: Token) {
        if self.scopes.len() >= MAX_DEPTH {
            self.model.report.problem(
                tag.line,
                "A2L nesting exceeds 128 blocks; remaining blocks skipped",
            );
            self.stopped = true;
            self.block = None;
            return;
        }
        self.start_row(&tag);
        if !matches!(tag.text.as_str(), "IF_DATA" | "A2ML") {
            self.append(&format!("/begin {}", tag.text), tag.line);
        }
        if tag.text == "IF_DATA"
            && let Some(module) = self.module()
        {
            self.model.modules[module].interface_data.present = true;
        }
        self.scopes.push(Scope {
            kind: tag.text,
            line: tag.line,
            module: None,
            named: false,
        });
    }

    fn start_row(&mut self, tag: &Token) {
        if self
            .scopes
            .iter()
            .any(|scope| matches!(scope.kind.as_str(), "IF_DATA" | "A2ML"))
        {
            return;
        }
        let is_row = matches!(
            tag.text.as_str(),
            "MEASUREMENT" | "CHARACTERISTIC" | "COMPU_METHOD"
        );
        if is_row && self.block.is_some() {
            self.model
                .report
                .problem(tag.line, "new row before the previous A2L row ended");
            self.block = None;
            if let Some(at) = self.scopes.iter().rposition(|scope| scope.kind == "MODULE") {
                self.scopes.truncate(at + 1);
            }
        }
        if is_row
            && self
                .scopes
                .last()
                .is_some_and(|scope| scope.kind == "MODULE")
        {
            self.block = Some(Block {
                kind: tag.text.clone(),
                depth: self.scopes.len(),
                line: tag.line,
                text: String::new(),
                overflow: false,
                last_line: tag.line,
                lines: vec![tag.line],
            });
        }
    }

    fn name(&mut self, token: &Token) {
        if token.kind != TokenKind::Word {
            return;
        }
        if self
            .scopes
            .iter()
            .take(self.scopes.len().saturating_sub(1))
            .any(|scope| matches!(scope.kind.as_str(), "IF_DATA" | "A2ML"))
        {
            return;
        }
        let Some(scope) = self.scopes.last() else {
            return;
        };
        if scope.named {
            return;
        }
        if scope.kind == "PROJECT"
            && self.model.project.is_none()
            && self.model.report.retain(token.line, token.text.len())
        {
            self.model.project = Some(token.text.clone());
        }
        if scope.kind == "MODULE"
            && scope.module.is_none()
            && self.model.report.retain(token.line, token.text.len())
        {
            let at = self.model.modules.len();
            self.model.modules.push(Module {
                name: token.text.clone(),
                line: token.line,
                interface_data: InterfaceData::default(),
            });
            if let Some(scope) = self.scopes.last_mut() {
                scope.module = Some(at);
            }
        }
        if scope_kind(&self.scopes) == Some("IF_DATA")
            && let Some(module) = self.module()
        {
            let data = &mut self.model.modules[module].interface_data;
            data.xcp |= token.text == "XCP";
            data.ccp |= token.text == "CCP";
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.named = true;
        }
    }

    fn end(&mut self, tag: Token) {
        let Some(at) = self.scopes.iter().rposition(|scope| scope.kind == tag.text) else {
            self.model
                .report
                .problem(tag.line, format!("unmatched /end {}", tag.text));
            return;
        };
        if at + 1 != self.scopes.len() {
            self.model
                .report
                .problem(tag.line, "A2L end tag does not match the open block");
        }
        let skip = matches!(tag.text.as_str(), "IF_DATA" | "A2ML");
        if !skip {
            self.append(&format!("/end {}", tag.text), tag.line);
        }
        if self
            .block
            .as_ref()
            .is_some_and(|block| block.kind == tag.text && block.depth == at)
            && let (Some(block), Some(module)) = (self.block.take(), self.module())
        {
            self.model.parse(block, module);
        }
        self.scopes.truncate(at);
    }

    fn token(&mut self, token: Token) {
        if token.kind == TokenKind::EndLine {
            return;
        }
        if token.kind == TokenKind::Problem {
            self.model.report.problem(token.line, token.text);
            if !self
                .scopes
                .iter()
                .any(|scope| matches!(scope.kind.as_str(), "IF_DATA" | "A2ML"))
                && let Some(block) = &mut self.block
            {
                block.overflow = true;
            }
            return;
        }
        self.name(&token);
        self.append(&token.text, token.line);
    }

    fn finish(&mut self) {
        for scope in &self.scopes {
            self.model
                .report
                .problem(scope.line, format!("unclosed A2L block {}", scope.kind));
        }
        if self.model.project.is_none() {
            self.model.report.problem(1, "missing A2L PROJECT");
        }
        self.model.resolve_units();
    }
}

fn scope_kind(scopes: &[Scope]) -> Option<&str> {
    scopes.last().map(|scope| scope.kind.as_str())
}

pub fn index_a2l(read: impl BufRead, watch: Watch<'_>) -> Result<A2lModel, DescriptionError> {
    let mut lexer = Lexer::new(read, watch);
    let mut scanner = Scanner::default();
    while let Some(token) = lexer.next()? {
        if scanner.stopped {
            continue;
        }
        match (token.kind, token.text.as_str()) {
            (TokenKind::Word, "/begin") => {
                if let Some(tag) = lexer.significant()? {
                    scanner.begin(tag);
                }
            }
            (TokenKind::Word, "/end") => {
                if let Some(tag) = lexer.significant()? {
                    scanner.end(tag);
                }
            }
            (TokenKind::Word, "/include") => {
                scanner.model.report.problem(
                    token.line,
                    "A2L includes are not loaded by the description index",
                );
                lexer.significant()?;
            }
            _ => scanner.token(token),
        }
    }
    scanner.finish();
    (scanner.model.report.bytes, scanner.model.report.lines) = lexer.position();
    lexer.finish()?;
    Ok(scanner.model)
}
