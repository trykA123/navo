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
    let prefix = short_prefix(prefix, budget, width - MIN_REST);
    let room = width - prefix.chars().count();
    fitted(prefix, cut_middle(rest, rest_len, room))
}

fn short_prefix(prefix: &str, budget: usize, most: usize) -> String {
    let (kind, module) = split_kind(prefix);
    let kind_len = kind.chars().count();
    let module_len = module.chars().count();
    if module_len <= budget && kind_len + module_len <= most {
        return prefix.to_string();
    }
    if budget >= SHORT_PREFIX && kind_len + SHORT_PREFIX <= most {
        let head: String = module.chars().take(2).collect();
        return format!("{kind}{head}….");
    }
    String::new()
}

fn split_kind(prefix: &str) -> (&str, &str) {
    let Some(body) = prefix.strip_prefix('.') else {
        return ("", prefix);
    };
    match body.split_once('.') {
        Some((kind, module)) if !kind.is_empty() && !module.is_empty() => {
            prefix.split_at(kind.len() + 2)
        }
        _ => ("", prefix),
    }
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
    name.split_at(kind.len() + module.len() + 3)
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
        assert_eq!(map_prefix(".text.Com.Com_Send"), (".text.Com.", "Com_Send"));
        assert_eq!(map_prefix(".text.main"), ("", ".text.main"));
        assert_eq!(map_prefix("text.Com.x"), ("", "text.Com.x"));
        assert_eq!(map_prefix(".text..x"), ("", ".text..x"));
    }

    #[test]
    fn map_names_keep_their_kind_when_the_module_is_shortened() {
        let (prefix, rest) = map_prefix(".rodata.Lamp_Control.Read");
        assert_eq!(
            joined(&fit_name(prefix, rest, 40)),
            ".rodata.Lamp_Control.Read"
        );
        let code = fit_name(".text.Lamp_Control.", "Read_Input_Values", 24);
        let data = fit_name(".rodata.Lamp_Control.", "Read_Input_Values", 24);
        assert_eq!(code.prefix, ".text.La….");
        assert_eq!(data.prefix, ".rodata.La….");
        assert_ne!(joined(&code), joined(&data));
        assert!(joined(&data).chars().count() <= 24);
        assert!(data.rest.starts_with('R') && data.rest.ends_with('s'));
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
