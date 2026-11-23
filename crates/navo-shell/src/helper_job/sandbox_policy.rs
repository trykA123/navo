//! The PDF helper's sandbox flags as plain numbers: the Job UI limits and the process mitigation
//! policy (S2b). `sandbox_win.rs` hands them to Windows; they live here so Linux tests check them.

/// `JOB_OBJECT_UILIMIT_*` bits (winnt.h), with the values windows-sys 0.61.2 uses.
mod ui {
    pub const HANDLES: u32 = 0x01;
    pub const READ_CLIPBOARD: u32 = 0x02;
    pub const WRITE_CLIPBOARD: u32 = 0x04;
    pub const SYSTEM_PARAMETERS: u32 = 0x08;
    pub const GLOBAL_ATOMS: u32 = 0x20;
    pub const DESKTOP: u32 = 0x40;
}

/// `PROCESS_CREATION_MITIGATION_POLICY_*_ALWAYS_ON` bits (winbase.h macros, which windows-sys
/// does not export): each policy owns two bits at its shift, and "always on" is the low one.
mod mitigation {
    pub const STRICT_HANDLE_CHECKS: u64 = 1 << 24;
    pub const EXTENSION_POINT_DISABLE: u64 = 1 << 32;
    pub const PROHIBIT_DYNAMIC_CODE: u64 = 1 << 36;
    pub const IMAGE_LOAD_NO_REMOTE: u64 = 1 << 52;
}

/// The Job UI limits: no other desktop, no clipboard read or write, no USER handles from outside
/// the Job, no global atoms and no system parameter changes.
pub const UI_LIMITS: u32 = ui::HANDLES
    | ui::READ_CLIPBOARD
    | ui::WRITE_CLIPBOARD
    | ui::SYSTEM_PARAMETERS
    | ui::GLOBAL_ATOMS
    | ui::DESKTOP;

/// The mitigations for `PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY`: strict handle checks, legacy
/// extension points off, no images from remote shares, no dynamic code.
pub const MITIGATIONS: u64 = mitigation::STRICT_HANDLE_CHECKS
    | mitigation::EXTENSION_POINT_DISABLE
    | mitigation::PROHIBIT_DYNAMIC_CODE
    | mitigation::IMAGE_LOAD_NO_REMOTE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_limits_are_the_five_planned_restrictions() {
        // HANDLES 0x01, READ 0x02, WRITE 0x04, SYSTEMPARAMETERS 0x08, GLOBALATOMS 0x20,
        // DESKTOP 0x40; DISPLAYSETTINGS 0x10 and EXITWINDOWS 0x80 are not in the plan.
        assert_eq!(UI_LIMITS, 0x6F);
    }

    #[test]
    fn mitigations_are_the_four_planned_always_on_bits() {
        assert_eq!(mitigation::STRICT_HANDLE_CHECKS, 1 << 24);
        assert_eq!(mitigation::EXTENSION_POINT_DISABLE, 1 << 32);
        assert_eq!(mitigation::PROHIBIT_DYNAMIC_CODE, 1 << 36);
        assert_eq!(mitigation::IMAGE_LOAD_NO_REMOTE, 1 << 52);
        assert_eq!(MITIGATIONS, 0x0010_0011_0100_0000);
    }

    #[test]
    fn no_mitigation_bit_is_an_always_off_bit() {
        // Each policy's "always off" bit sits one above its "always on" bit.
        assert_eq!(MITIGATIONS & (MITIGATIONS << 1), 0);
        assert_eq!(MITIGATIONS & (MITIGATIONS >> 1), 0);
    }
}
