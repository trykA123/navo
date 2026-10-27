use super::Axis;

pub const MAX_PANES: usize = 4;
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tiles {
    Pane,
    Split {
        axis: Axis,
        percent: u8,
        first: Box<Self>,
        second: Box<Self>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub tiles: Tiles,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message} at column {column}")]
pub struct TilesError {
    pub column: usize,
    message: &'static str,
}

impl Tiles {
    pub fn parse(text: &str) -> Result<Self, TilesError> {
        let mut parser = Parser {
            text,
            pos: 0,
            panes: 0,
        };
        let tiles = parser.tiles(0)?;
        parser.skip_ws();
        if parser.pos != text.len() {
            return Err(parser.error("unexpected text"));
        }
        Ok(tiles)
    }

    #[must_use]
    pub fn print(&self) -> String {
        match self {
            Self::Pane => "pane".to_owned(),
            Self::Split {
                axis,
                percent,
                first,
                second,
            } => {
                let name = match axis {
                    Axis::SideBySide => "row",
                    Axis::Stacked => "col",
                };
                format!("{name}({percent}, {}, {})", first.print(), second.print())
            }
        }
    }

    #[must_use]
    pub fn builtin() -> [Preset; 5] {
        use Axis::{SideBySide as Row, Stacked as Col};
        let split = |axis, percent, first, second| Self::Split {
            axis,
            percent,
            first: Box::new(first),
            second: Box::new(second),
        };
        [
            Preset {
                name: "Side by side",
                tiles: split(Row, 50, Self::Pane, Self::Pane),
            },
            Preset {
                name: "Three columns",
                tiles: split(Row, 33, Self::Pane, split(Row, 50, Self::Pane, Self::Pane)),
            },
            Preset {
                name: "Wide + 2",
                tiles: split(Col, 52, Self::Pane, split(Row, 50, Self::Pane, Self::Pane)),
            },
            Preset {
                name: "Two by two",
                tiles: split(
                    Row,
                    50,
                    split(Col, 50, Self::Pane, Self::Pane),
                    split(Col, 50, Self::Pane, Self::Pane),
                ),
            },
            Preset {
                name: "Main + 3",
                tiles: split(
                    Row,
                    60,
                    Self::Pane,
                    split(Col, 33, Self::Pane, split(Col, 50, Self::Pane, Self::Pane)),
                ),
            },
        ]
    }
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
    panes: usize,
}

impl Parser<'_> {
    fn tiles(&mut self, depth: usize) -> Result<Tiles, TilesError> {
        self.skip_ws();
        if depth > MAX_DEPTH {
            return Err(self.error("layout depth exceeds 8"));
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_lowercase()) {
            self.pos += 1;
        }
        match &self.text[start..self.pos] {
            "pane" => {
                self.panes += 1;
                if self.panes > MAX_PANES {
                    return Err(self.error_at(start, "layout exceeds four panes"));
                }
                Ok(Tiles::Pane)
            }
            "row" => self.split(Axis::SideBySide, depth),
            "col" => self.split(Axis::Stacked, depth),
            _ => Err(self.error_at(start, "expected pane, row or col")),
        }
    }

    fn split(&mut self, axis: Axis, depth: usize) -> Result<Tiles, TilesError> {
        self.eat('(')?;
        let percent = self.percent()?;
        self.eat(',')?;
        let first = Box::new(self.tiles(depth + 1)?);
        self.eat(',')?;
        let second = Box::new(self.tiles(depth + 1)?);
        self.eat(')')?;
        Ok(Tiles::Split {
            axis,
            percent,
            first,
            second,
        })
    }

    fn percent(&mut self) -> Result<u8, TilesError> {
        self.skip_ws();
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.text[start..self.pos]
            .parse::<u8>()
            .ok()
            .filter(|percent| (10..=90).contains(percent))
            .ok_or_else(|| self.error_at(start, "expected percent from 10 to 90"))
    }

    fn eat(&mut self, expected: char) -> Result<(), TilesError> {
        self.skip_ws();
        if self.peek() != Some(expected) {
            return Err(self.error(match expected {
                '(' => "expected '('",
                ')' => "expected ')'",
                _ => "expected ','",
            }));
        }
        self.pos += expected.len_utf8();
        Ok(())
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn skip_ws(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
        {
            self.pos += 1;
        }
    }

    fn error(&self, message: &'static str) -> TilesError {
        self.error_at(self.pos, message)
    }

    fn error_at(&self, pos: usize, message: &'static str) -> TilesError {
        TilesError {
            column: self.text[..pos].chars().count() + 1,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::layout::{Axis, MAX_PANES, Tiles};

    #[test]
    fn builtins_parse_and_print_unchanged() {
        let expected = [
            ("Side by side", "row(50, pane, pane)"),
            ("Three columns", "row(33, pane, row(50, pane, pane))"),
            ("Wide + 2", "col(52, pane, row(50, pane, pane))"),
            (
                "Two by two",
                "row(50, col(50, pane, pane), col(50, pane, pane))",
            ),
            (
                "Main + 3",
                "row(60, pane, col(33, pane, col(50, pane, pane)))",
            ),
        ];
        for (preset, (name, notation)) in Tiles::builtin().into_iter().zip(expected) {
            assert_eq!(preset.name, name);
            assert_eq!(preset.tiles.print(), notation);
            assert_eq!(Tiles::parse(notation).unwrap(), preset.tiles);
        }
    }

    #[test]
    fn whitespace_variants_parse_equal() {
        let expected = Tiles::parse("row(50, pane, col(10, pane, pane))").unwrap();
        for text in [
            " row ( 50 , pane , col ( 10 , pane , pane ) ) ",
            "\trow(50,pane,\ncol(10,\rpane,pane))\n",
        ] {
            assert_eq!(Tiles::parse(text).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_notation_reports_the_offending_column() {
        for (text, column) in [
            (
                "row(50, pane, row(50, pane, row(50, pane, row(50, pane, pane))))",
                57,
            ),
            ("row(95, pane, pane)", 5),
            ("row(50, pane)", 13),
            ("grid", 1),
            ("pane extra", 6),
            ("", 1),
            ("row(-10, pane, pane)", 5),
            ("row(99999999999999999999, pane, pane)", 5),
            ("row(50, pane, pane", 19),
            ("row(50, pane, pane, pane)", 19),
            ("paneé", 5),
        ] {
            let error = Tiles::parse(text).unwrap_err();
            assert_eq!(error.column, column, "{text}");
            assert!(error.to_string().contains(&format!("column {column}")));
        }
    }

    #[test]
    fn pane_and_percent_boundaries_parse() {
        assert_eq!(MAX_PANES, 4);
        assert_eq!(Tiles::parse("pane").unwrap(), Tiles::Pane);
        for percent in [10, 90] {
            assert_eq!(
                Tiles::parse(&format!("col({percent}, pane, pane)")).unwrap(),
                Tiles::Split {
                    axis: Axis::Stacked,
                    percent,
                    first: Box::new(Tiles::Pane),
                    second: Box::new(Tiles::Pane),
                }
            );
        }
        for percent in [0, 9, 91, 100, 256] {
            assert_eq!(
                Tiles::parse(&format!("row({percent}, pane, pane)"))
                    .unwrap_err()
                    .column,
                5
            );
        }
    }

    #[test]
    fn excessive_depth_is_rejected_before_descending() {
        let text = format!("{}pane{}", "row(50, ".repeat(9), ", pane)".repeat(9));
        let error = Tiles::parse(&text).unwrap_err();
        assert_eq!(error.column, 73);
        assert!(error.to_string().contains("depth"));
    }
}
