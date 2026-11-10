//! Builds crash-hook JSON summaries from native minidumps without paths or stack contents.

use minidump::{
    Minidump, MinidumpContext, MinidumpException, MinidumpMiscInfo, MinidumpModuleList,
    MinidumpSystemInfo, MinidumpThreadList, Module, UnifiedMemory,
};

#[derive(Debug)]
pub(super) struct NativeCrash {
    pub(super) reason: String,
    pub(super) at: Option<String>,
    pub(super) frames: Vec<String>,
}

/// Summarizes minidump bytes without file paths; fails for an unreadable dump.
pub(super) fn summarize(dump: &[u8]) -> Result<NativeCrash, minidump::Error> {
    let dump = Minidump::read(dump)?;
    let exception = dump.get_stream::<MinidumpException>()?;
    let modules = dump.get_stream::<MinidumpModuleList>().unwrap_or_default();
    let system = dump.get_stream::<MinidumpSystemInfo>().ok();
    let reason = system.as_ref().map_or_else(
        || {
            format!(
                "exception 0x{:08x}",
                exception.raw.exception_record.exception_code
            )
        },
        |system| {
            exception
                .get_crash_reason(system.os, system.cpu)
                .to_string()
        },
    );
    let mut crash = NativeCrash {
        reason,
        at: Some(frame(
            &modules,
            exception.raw.exception_record.exception_address,
        )),
        frames: Vec::new(),
    };
    let Some(system) = system else {
        return Ok(crash);
    };
    let Ok(threads) = dump.get_stream::<MinidumpThreadList>() else {
        return Ok(crash);
    };
    let Some(thread) = threads.get_thread(exception.get_crashing_thread_id()) else {
        return Ok(crash);
    };
    let misc = dump.get_stream::<MinidumpMiscInfo>().ok();
    let context = exception
        .context(&system, misc.as_ref())
        .or_else(|| thread.context(&system, misc.as_ref()));
    if let Some(context) = context {
        let memory = dump.get_memory().unwrap_or_default();
        crash.frames = scan_stack(&context, thread.stack_memory(&memory), &modules);
    }
    Ok(crash)
}

/// Formats an address as a module-relative frame, or raw hex outside modules.
fn frame(modules: &MinidumpModuleList, address: u64) -> String {
    let Some(module) = modules.module_at_address(address) else {
        return format!("0x{address:x}");
    };
    let Some(offset) = address.checked_sub(module.base_address()) else {
        return format!("0x{address:x}");
    };
    let name = module.code_file();
    let name = name.rsplit(['\\', '/']).next().unwrap_or_default();
    format!("{name}+0x{offset:x}")
}

/// Scans at most 64 KiB from the stack pointer, returning at most 32 frames, IP first.
fn scan_stack(
    context: &MinidumpContext,
    stack: Option<UnifiedMemory<'_, '_>>,
    modules: &MinidumpModuleList,
) -> Vec<String> {
    let mut frames = vec![frame(modules, context.get_instruction_pointer())];
    let Some(stack) = stack else { return frames };
    let pointer = context.get_stack_pointer();
    let Some(aligned) = pointer.checked_add(7).map(|address| address & !7) else {
        return frames;
    };
    let words = (64 * 1024 - (aligned - pointer)) / 8;
    for word in 0..words {
        let Some(address) = aligned.checked_add(word * 8) else {
            break;
        };
        let Some(address) = stack.get_memory_at_address::<u64>(address) else {
            break;
        };
        if modules.module_at_address(address).is_some() {
            frames.push(frame(modules, address));
            if frames.len() == 32 {
                break;
            }
        }
    }
    frames
}

/// Converts a native summary to the panic hook's record format without events.
pub(super) fn record(crash: &NativeCrash) -> super::hook::Record {
    let mut backtrace = String::from("stack scan (may include stale frames)");
    for frame in &crash.frames {
        backtrace.push('\n');
        backtrace.push_str(frame);
    }
    super::hook::Record {
        version: env!("CARGO_PKG_VERSION"),
        commit: env!("NAVO_COMMIT"),
        thread: "native".into(),
        location: crash.at.clone(),
        message: format!("native crash: {}", crash.reason),
        backtrace,
        events: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use minidump_synth::{
        DumpString, Exception, Memory, Module, SynthMinidump, SystemInfo, Thread, amd64_context,
    };
    use test_assembler::{Endian, Section};

    use super::*;

    const BASE: u64 = 0x1_4000_0000;
    const STACK: u64 = 0x20_0000;
    const MODULE: &str = r"C:\Users\someone\AppData\Local\navo\current\navo.exe";

    /// Builds a Windows AMD64 crash with a second thread and caller-supplied stack words.
    fn dump(module: &str, words: &[u64]) -> Vec<u8> {
        let endian = Endian::Little;
        let name = DumpString::new(module, endian);
        let module = Module::new(endian, BASE, 0x10000, &name, 0, 0, None);
        let context = amd64_context(endian, BASE + 0x1234, STACK + 8);
        let mut stack = Section::with_endian(endian).D64(BASE + 0xffff);
        for word in words {
            stack = stack.D64(*word);
        }
        let stack = Memory::with_section(stack, STACK);
        let thread = Thread::new(endian, 42, &stack, &context);
        let other_context = amd64_context(endian, BASE + 0xeeee, STACK);
        let other = Thread::new(endian, 7, &stack, &other_context);
        let mut exception = Exception::new(endian);
        exception.thread_id = 42;
        exception.exception_record.exception_code = 0xc0000005;
        exception.exception_record.exception_address = BASE + 0x1234;
        exception.exception_record.number_parameters = 2;
        exception.exception_record.exception_information[1] = 0xdeadbeef;
        SynthMinidump::with_endian(endian)
            .add_system_info(
                SystemInfo::new(endian)
                    .set_processor_architecture(9)
                    .set_platform_id(2),
            )
            .add_module(module)
            .add(name)
            .add_thread(other)
            .add_thread(thread)
            .add(other_context)
            .add(context)
            .add_memory(stack)
            .add_exception(exception)
            .finish()
            .unwrap()
    }

    #[test]
    fn access_violation_record_contains_no_user_name_or_path() {
        let crash = summarize(&dump(MODULE, &[BASE + 0x5678])).unwrap();
        assert!(
            crash.reason.contains("ACCESS_VIOLATION"),
            "{}",
            crash.reason
        );
        let record = record(&crash);
        let json = serde_json::to_string(&record).unwrap();
        assert!(!json.contains("someone"), "summary must omit the user name");
        assert!(!json.contains(r":\"), "summary must omit drive separators");
        assert_eq!(crash.at.as_deref(), Some("navo.exe+0x1234"));
        assert_eq!(crash.frames, ["navo.exe+0x1234", "navo.exe+0x5678"]);
        assert_eq!(record.thread, "native");
        assert_eq!(record.location, crash.at);
        assert_eq!(record.message, format!("native crash: {}", crash.reason));
        assert_eq!(
            record.backtrace,
            "stack scan (may include stale frames)\nnavo.exe+0x1234\nnavo.exe+0x5678"
        );
        assert!(record.events.is_empty());
        assert_eq!(record.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(record.commit, env!("NAVO_COMMIT"));
    }

    #[test]
    fn stack_scan_filters_noise_and_caps_frames_at_32() {
        let words: Vec<_> = (0..40)
            .flat_map(|offset| [0, BASE + offset, u64::MAX])
            .collect();
        let crash = summarize(&dump("/home/someone/navo.exe", &words)).unwrap();
        assert_eq!(crash.frames.len(), 32);
        assert_eq!(crash.frames[0], "navo.exe+0x1234");
        for (frame, offset) in crash.frames[1..].iter().zip(0..31) {
            assert_eq!(frame, &format!("navo.exe+0x{offset:x}"));
        }
    }

    #[test]
    fn garbage_bytes_return_an_error_without_panicking() {
        for bytes in [&b"garbage"[..], &[], &b"MDMP"[..]] {
            assert!(summarize(bytes).is_err(), "invalid dumps must return Err");
        }
    }
}
