use std::fmt::{self, Write};

const MAX_MANGLED: usize = 1024;
const MAX_DEMANGLED: usize = 2048;
const RECURSION: u32 = 128;

struct Bounded(String);

impl Write for Bounded {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.0.len() + text.len() > MAX_DEMANGLED {
            return Err(fmt::Error);
        }
        self.0.push_str(text);
        Ok(())
    }
}

pub(super) fn demangle(name: &str) -> Option<String> {
    if name.len() > MAX_MANGLED {
        return None;
    }
    if (name.starts_with("_R") || name.starts_with("_ZN"))
        && let Ok(rust) = rustc_demangle::try_demangle(name)
    {
        let mut out = Bounded(String::new());
        return write!(out, "{rust:#}").ok().map(|()| out.0);
    }
    if !name.starts_with("_Z") {
        return None;
    }
    let parse = cpp_demangle::ParseOptions::default().recursion_limit(RECURSION);
    let symbol = cpp_demangle::Symbol::new_with_options(name.as_bytes(), &parse).ok()?;
    let options = cpp_demangle::DemangleOptions::new().recursion_limit(RECURSION);
    let mut out = Bounded(String::new());
    symbol.structured_demangle(&mut out, &options).ok()?;
    Some(out.0)
}

#[cfg(test)]
mod tests {
    use super::demangle;

    #[test]
    fn c_names_stay_as_they_are() {
        assert_eq!(demangle("main"), None);
        assert_eq!(demangle("_start"), None);
    }

    #[test]
    fn cpp_names_are_demangled() {
        assert_eq!(demangle("_Z3addii").as_deref(), Some("add(int, int)"));
    }

    #[test]
    fn rust_names_are_demangled_without_the_hash() {
        assert_eq!(
            demangle("_ZN4core3fmt5write17h0123456789abcdefE").as_deref(),
            Some("core::fmt::write")
        );
    }

    #[test]
    fn broken_mangled_names_stay_raw() {
        assert_eq!(demangle("_Zzzz"), None);
    }
}
