const MIN_CHARS: usize = 6;
const PREFIX_SHARE: usize = 30;
const HEAD_SHARE: usize = 45;
const SHORT_PREFIX: usize = 4;
const MIN_REST: usize = 3;
const SYMBOL_PREFIX_CHARS: std::ops::RangeInclusive<usize> = 2..=16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted {
    pub prefix: String,
    pub rest: String,
}

pub fn fit_name(prefix: &str, rest: &str, chars: usize) -> Fitted {
    let width = chars.max(MIN_CHARS);
    let prefix_len = prefix.chars().count();
    let rest_len = rest.chars().count();
    if prefix_len + rest_len <= width {
        return fitted(prefix.to_string(), rest.to_string());
    }
    let budget = (width * PREFIX_SHARE / 100).min(width - MIN_REST);
    let prefix = if prefix_len <= budget {
        prefix.to_string()
    } else if budget >= SHORT_PREFIX {
        let head: String = prefix.chars().take(2).collect();
        format!("{head}….")
    } else {
        String::new()
    };
    let room = width - prefix.chars().count();
    fitted(prefix, cut_middle(rest, rest_len, room))
}

fn fitted(prefix: String, rest: String) -> Fitted {
    Fitted { prefix, rest }
}

fn cut_middle(text: &str, len: usize, room: usize) -> String {
    if len <= room {
        return text.to_string();
    }
    let keep = room - 1;
    let head = (keep * HEAD_SHARE).div_ceil(100).clamp(1, keep - 1);
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(len - tail).collect();
    format!("{start}…{end}")
}

pub fn map_prefix(name: &str) -> (&str, &str) {
    let Some(body) = name.strip_prefix('.') else {
        return ("", name);
    };
    let Some((kind, tail)) = body.split_once('.') else {
        return ("", name);
    };
    let Some((module, rest)) = tail.split_once('.') else {
        return ("", name);
    };
    if kind.is_empty() || module.is_empty() || rest.is_empty() {
        return ("", name);
    }
    let start = 2 + kind.len();
    (&name[start..start + module.len() + 1], rest)
}

pub fn symbol_prefix(name: &str) -> (&str, &str) {
    let Some(at) = name.find('_') else {
        return ("", name);
    };
    let end = at + 1;
    let chars = name[..end].chars().count();
    if SYMBOL_PREFIX_CHARS.contains(&chars) && end < name.len() {
        name.split_at(end)
    } else {
        ("", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn joined(fitted: &Fitted) -> String {
        format!("{}{}", fitted.prefix, fitted.rest)
    }

    #[test]
    fn fits_unchanged() {
        let out = fit_name("Com_", "Send", 20);
        assert_eq!(joined(&out), "Com_Send");
        assert_eq!(out.prefix, "Com_");
    }

    #[test]
    fn long_rest_is_cut_in_the_middle_at_45_percent() {
        let out = fit_name("Com_", "abcdefghijklmnopqrstuvwxyz", 20);
        assert_eq!(out.prefix, "Com_");
        assert_eq!(out.rest, "abcdefg…stuvwxyz");
        assert_eq!(joined(&out).chars().count(), 20);
    }

    #[test]
    fn long_prefix_is_shortened() {
        let out = fit_name("Autosar_Module_", "Init_Function_Name", 20);
        assert_eq!(out.prefix, "Au….");
        assert!(joined(&out).chars().count() <= 20);
        assert!(out.rest.contains('…'));
    }

    #[test]
    fn minimum_is_six_characters() {
        let out = fit_name("", "abcdefghij", 2);
        assert_eq!(out.rest.chars().count(), 6);
        assert!(out.rest.starts_with('a') && out.rest.ends_with('j'));
    }

    #[test]
    fn tight_width_drops_the_prefix() {
        let out = fit_name("Module_", "abcdefghij", 6);
        assert_eq!(out.prefix, "");
        assert_eq!(out.rest.chars().count(), 6);
    }

    #[test]
    fn non_ascii_names_are_cut_on_code_points() {
        let out = fit_name("Ü_", "äöüßäöüßäöüßäöüß", 8);
        assert_eq!(out.prefix, "Ü_");
        assert_eq!(out.rest, "äöü…üß");
    }

    #[test]
    fn map_prefix_takes_the_module() {
        assert_eq!(map_prefix(".text.Com.Com_Send"), ("Com.", "Com_Send"));
        assert_eq!(map_prefix(".text.main"), ("", ".text.main"));
        assert_eq!(map_prefix("text.Com.x"), ("", "text.Com.x"));
        assert_eq!(map_prefix(".text..x"), ("", ".text..x"));
    }

    #[test]
    fn symbol_prefix_needs_two_to_sixteen_chars() {
        assert_eq!(symbol_prefix("Com_Send"), ("Com_", "Send"));
        assert_eq!(symbol_prefix("_start"), ("", "_start"));
        assert_eq!(symbol_prefix("a_b"), ("a_", "b"));
        assert_eq!(symbol_prefix("main"), ("", "main"));
        assert_eq!(symbol_prefix("Com_"), ("", "Com_"));
        let long = "abcdefghijklmnopq_x";
        assert_eq!(symbol_prefix(long), ("", long));
        assert_eq!(symbol_prefix("Üb_x"), ("Üb_", "x"));
    }

    proptest! {
        #[test]
        fn result_is_bounded_and_keeps_the_rest_ends(
            prefix in "\\PC{0,30}",
            rest in "\\PC{1,80}",
            chars in 0usize..60,
        ) {
            let out = fit_name(&prefix, &rest, chars);
            let total = out.prefix.chars().count() + out.rest.chars().count();
            prop_assert!(total <= chars.max(MIN_CHARS));
            prop_assert_eq!(out.rest.chars().next(), rest.chars().next());
            prop_assert_eq!(out.rest.chars().last(), rest.chars().last());
        }
    }
}
