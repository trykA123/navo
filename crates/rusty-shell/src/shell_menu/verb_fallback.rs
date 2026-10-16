const E_NOTIMPL: i32 = 0x8000_4001_u32.cast_signed();
const E_INVALIDARG: i32 = 0x8007_0057_u32.cast_signed();

pub(super) fn is_unknown_verb(code: i32) -> bool {
    matches!(code, E_NOTIMPL | E_INVALIDARG)
}

#[cfg(test)]
mod tests {
    use super::is_unknown_verb;

    fn from_win32(error: u32) -> i32 {
        (0x8007_0000 | error).cast_signed()
    }

    #[test]
    fn not_implemented_means_an_unknown_verb() {
        assert!(is_unknown_verb(0x8000_4001_u32.cast_signed()));
    }

    #[test]
    fn invalid_argument_means_an_unknown_verb() {
        assert!(is_unknown_verb(0x8007_0057_u32.cast_signed()));
    }

    #[test]
    fn win32_invalid_parameter_means_an_unknown_verb() {
        assert!(is_unknown_verb(from_win32(87)));
    }

    #[test]
    fn a_cancelled_command_is_not_retried() {
        assert!(!is_unknown_verb(from_win32(1223)));
    }

    #[test]
    fn other_failures_are_not_retried() {
        for code in [
            0x8000_4005_u32.cast_signed(),
            from_win32(5),
            from_win32(2),
            0,
        ] {
            assert!(!is_unknown_verb(code), "{code:#x}");
        }
    }
}
