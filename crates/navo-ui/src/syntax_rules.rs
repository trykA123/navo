//! Rule-language state and the line-lexer adapter over `navo-syntax`.
//! It sits between the loaded rule registry and the paged/editor line lexers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use navo_syntax::{Language, Lexer, Registry};
use sidewise_kit::languages::lines::{LineLexer, LineState, Span};

mod load;
mod state;
pub use load::{RuleDir, load_rules};

static INSTALLED: OnceLock<Registry> = OnceLock::new();
static BUILTIN: OnceLock<Registry> = OnceLock::new();

/// Installs the loaded rule registry; the first call wins and returns true.
pub fn install_rules(registry: Registry) -> bool {
    INSTALLED.set(registry).is_ok()
}

/// Returns the installed rule registry, else the built-in one.
pub fn rules() -> &'static Registry {
    INSTALLED
        .get()
        .unwrap_or_else(|| BUILTIN.get_or_init(Registry::builtin))
}

pub struct RuleLexer {
    name: &'static str,
    language: &'static Language,
    lexer: Lexer,
}

impl RuleLexer {
    /// Builds a line lexer for a language that lives for the whole process.
    pub fn new(language: &'static Language) -> Self {
        Self {
            name: language.name.as_str(),
            language,
            lexer: Lexer::new(language),
        }
    }
}

impl RuleLexer {
    /// Returns the one compiled lexer of a language, building it on first use.
    ///
    /// Registry languages live for the whole process, so the language's address names it. A
    /// built-in and an installed language with one name stay two lexers.
    pub fn shared(language: &'static Language) -> Arc<Self> {
        static COMPILED: OnceLock<Mutex<HashMap<usize, Arc<RuleLexer>>>> = OnceLock::new();
        let mut compiled = COMPILED
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let key = std::ptr::from_ref(language).addr();
        Arc::clone(
            compiled
                .entry(key)
                .or_insert_with(|| Arc::new(Self::new(language))),
        )
    }
}

impl LineLexer for RuleLexer {
    fn name(&self) -> &str {
        self.name
    }

    fn identity(&self) -> usize {
        std::ptr::from_ref(self.language).addr()
    }

    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.lex_chunk(chunk, state, false)
    }

    fn lex_last(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.lex_chunk(chunk, state, true)
    }

    fn line_end(&self, state: LineState) -> LineState {
        self.end_of_line(state)
    }
}

#[cfg(test)]
mod split_tests;

#[cfg(test)]
mod tests {
    use gpui_kit::component::highlighter::HighlightTheme;
    use navo_syntax::{ChunkState, Inside, Language, TokenKind};
    use sidewise_kit::languages::lines;

    use super::state::{pack, unpack};
    use super::*;
    use crate::tokens::{ColorTheme, Mode, SyntaxTheme, Theme, ThemeLook, highlight_theme};

    const KINDS: [TokenKind; 13] = [
        TokenKind::Keyword,
        TokenKind::Type,
        TokenKind::Function,
        TokenKind::Constant,
        TokenKind::Attribute,
        TokenKind::Tag,
        TokenKind::Property,
        TokenKind::Variable,
        TokenKind::Comment,
        TokenKind::String,
        TokenKind::Number,
        TokenKind::Operator,
        TokenKind::Delimiter,
    ];

    const FOO: &str = "[[language]]\nname = \"foo\"\nextensions = [\"foo\"]\n\
        [[language.keywords]]\nkind = \"keyword\"\nwords = [\"FOO\"]\n";

    /// Returns the built-in `oil` language from `rules()`.
    fn oil() -> &'static Language {
        rules().for_name("x.oil").expect("built-in oil language")
    }

    /// Leaks a test language parsed from TOML so it can be borrowed for the process.
    fn leaked(toml: &str) -> &'static Language {
        let mut languages = navo_syntax::from_toml(toml).expect("test language parses");
        Box::leak(Box::new(languages.remove(0)))
    }

    /// Builds one kit highlight theme over the default colour theme.
    fn built(syntax: SyntaxTheme, mode: Mode) -> HighlightTheme {
        let look = ThemeLook {
            color_theme: ColorTheme::default(),
            ..ThemeLook::default()
        };
        let theme = Theme::with_look(mode, look);
        let base = HighlightTheme::default_light();
        highlight_theme(syntax, mode, &theme.colors, &base)
    }

    /// A chunk state with the given position and word flag.
    fn at(inside: Inside, mid_word: bool) -> ChunkState {
        ChunkState {
            inside,
            mid_word,
            partial: 0,
        }
    }

    #[test]
    fn chunk_state_packs_into_a_line_state_and_back() {
        let insides = [
            Inside::Code,
            Inside::LineComment,
            Inside::Block(0),
            Inside::Block(u16::MAX),
            Inside::Delimited {
                index: 3,
                escaped: false,
            },
            Inside::Delimited {
                index: u16::MAX,
                escaped: true,
            },
        ];
        for inside in insides {
            for mid_word in [false, true] {
                let state = at(inside, mid_word && inside == Inside::Code);
                assert_eq!(unpack(pack(state)), state, "{state:?}");
            }
        }
        assert_eq!(pack(at(Inside::Code, false)), LineState(0));
        assert_eq!(pack(at(Inside::Block(2), false)), LineState(0x1_0002));
        let inside = Inside::Delimited {
            index: 2,
            escaped: false,
        };
        assert_eq!(pack(at(inside, false)), LineState(0x2_0002));
        assert_eq!(pack(at(Inside::LineComment, false)), LineState(0x3_0000));
        let escaped = Inside::Delimited {
            index: 2,
            escaped: true,
        };
        assert_eq!(pack(at(escaped, false)), LineState(0x4_0002));
        assert_eq!(pack(at(Inside::Code, true)), LineState(0x10_0000));
        assert_eq!(unpack(LineState(9)), at(Inside::Code, false));
        assert_eq!(unpack(LineState(0x3_0001)), at(Inside::LineComment, false));
        assert_eq!(unpack(LineState(0x1_0000)), at(Inside::Block(0), false));
    }

    #[test]
    fn a_held_pattern_length_packs_without_aliasing() {
        let insides = [
            Inside::Opening(u16::MAX - 1),
            Inside::Block(0),
            Inside::Delimited {
                index: 7,
                escaped: false,
            },
        ];
        for partial in [1, 2, 63] {
            for inside in insides {
                let mid_word = matches!(inside, Inside::Opening(_));
                let state = ChunkState {
                    inside,
                    mid_word,
                    partial,
                };
                assert_eq!(unpack(pack(state)), state, "{state:?}");
            }
        }
        let held = ChunkState {
            inside: Inside::Block(1),
            mid_word: false,
            partial: 3,
        };
        let line = RuleLexer::new(oil()).end_of_line(pack(held));
        assert_eq!(
            unpack(line),
            at(Inside::Block(1), false),
            "the next line holds nothing"
        );
    }

    #[test]
    fn rules_before_install_has_oil_and_install_keeps_the_first() {
        assert!(rules().for_name("a.oil").is_some());
        let mut loaded = Registry::builtin();
        let foo = navo_syntax::from_toml(FOO).expect("foo parses").remove(0);
        loaded.add(foo);
        assert!(install_rules(loaded));
        assert!(rules().for_name("a.foo").is_some());
        assert!(rules().for_name("a.oil").is_some());
        assert!(!install_rules(Registry::default()));
        assert!(rules().for_name("a.foo").is_some());
    }

    #[test]
    fn lexes_task_as_a_keyword_relative_to_the_chunk() {
        let lexer = RuleLexer::new(oil());
        assert_eq!(lexer.name(), "oil");
        assert!(!lexer.line_local());
        let (spans, state) = lexer.lex("x = TASK", LineState(0));
        assert_eq!(spans, [(2..3, "operator"), (4..8, "keyword")]);
        assert_eq!(
            state,
            pack(at(Inside::Code, true)),
            "the chunk ends in a word"
        );
        assert_eq!(
            lexer.lex_last("x = TASK", LineState(0)),
            (spans, LineState(0))
        );
        assert_eq!(lexer.line_end(state), LineState(0));
    }

    #[test]
    fn a_block_comment_keeps_its_state_across_lines() {
        let lexer = RuleLexer::new(oil());
        let (first, state) = lexer.lex("CPU /* open", LineState(0));
        assert_eq!(first, [(0..3, "keyword"), (4..11, "comment")]);
        assert_eq!(state, pack(at(Inside::Block(0), false)));
        assert_eq!(lexer.line_end(state), state);
        let (second, end) = lexer.lex("close */ TASK", state);
        assert_eq!(second, [(0..8, "comment"), (9..13, "keyword")]);
        assert_eq!(lexer.line_end(end), LineState(0));
    }

    #[test]
    fn a_long_chunk_is_clipped_to_max_spans() {
        let language = leaked(
            "[[language]]\nname = \"cap\"\nextensions = [\"cap\"]\n\
             [[language.keywords]]\nkind = \"keyword\"\nwords = [\"a\"]\n",
        );
        let lexer = RuleLexer::new(language);
        let chunk = "a ".repeat(lines::MAX_SPANS + 1);
        let (spans, state) = lexer.lex(&chunk, LineState(0));
        assert_eq!(spans.len(), lines::MAX_SPANS);
        assert_eq!(spans.last(), Some(&(8190..8191, "keyword")));
        assert_eq!(state, LineState(0));
    }

    /// Asserts every role a rule token kind can emit is in the `ROLES` contract (D5) and that
    /// the kit theme colours it.
    fn assert_known_roles(highlight: &HighlightTheme) {
        for kind in KINDS {
            let name = kind.highlight_name();
            assert!(
                lines::ROLES.contains(&name),
                "{name} is not in lines::ROLES"
            );
            assert!(
                highlight.style(name).is_some(),
                "{name} has no kit highlight name"
            );
        }
    }

    #[test]
    fn every_rule_role_is_a_kit_highlight_name() {
        for syntax in SyntaxTheme::ALL {
            for mode in [Mode::Light, Mode::Dark] {
                assert_known_roles(&built(syntax, mode));
            }
        }
    }
}
