use std::{io, ptr};

use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, QUERY_SERVICE_CONFIGW, QueryServiceConfig2W,
    QueryServiceConfigW, SC_ACTION, SERVICE_CONFIG, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONFIG_DESCRIPTION, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_CONFIG_FAILURE_ACTIONS_FLAG, SERVICE_FAILURE_ACTIONSW,
};

use super::super::scm::{ServiceHandle, checked};

pub(super) struct Configuration {
    basic: Vec<usize>,
    optional: Vec<(SERVICE_CONFIG, Vec<usize>)>,
}

impl Configuration {
    pub fn read(service: &ServiceHandle) -> io::Result<Self> {
        let basic = query(|buffer, size, needed| {
            // SAFETY: query supplies aligned writable storage and a writable byte count.
            unsafe { QueryServiceConfigW(service.0, buffer.cast(), size, needed) }
        })?;
        let mut optional = Vec::new();
        for kind in [
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            SERVICE_CONFIG_DESCRIPTION,
        ] {
            let data = query(|buffer, size, needed| {
                // SAFETY: query supplies aligned writable storage matching this information level.
                unsafe { QueryServiceConfig2W(service.0, kind, buffer, size, needed) }
            })?;
            optional.push((kind, data));
        }
        Ok(Self { basic, optional })
    }

    pub fn restore(&self, service: &ServiceHandle) -> io::Result<()> {
        // SAFETY: QueryServiceConfigW initialized this aligned buffer and all embedded pointers remain live.
        let basic = unsafe { &*self.basic.as_ptr().cast::<QUERY_SERVICE_CONFIGW>() };
        // SAFETY: service and the captured configuration remain live through the synchronous call.
        let mut result = checked(unsafe {
            ChangeServiceConfigW(
                service.0,
                basic.dwServiceType,
                basic.dwStartType,
                basic.dwErrorControl,
                basic.lpBinaryPathName,
                ptr::null(),
                ptr::null_mut(),
                ptr::null(),
                basic.lpServiceStartName,
                ptr::null(),
                ptr::null(),
            )
        });
        for (kind, data) in &self.optional {
            let restored = restore_optional(service, *kind, data);
            result = result.and(restored);
        }
        result
    }
}

fn restore_optional(
    service: &ServiceHandle,
    kind: SERVICE_CONFIG,
    data: &[usize],
) -> io::Result<()> {
    if kind == SERVICE_CONFIG_FAILURE_ACTIONS {
        // SAFETY: this level was initialized by QueryServiceConfig2W; its embedded pointers stay live.
        let mut actions = unsafe { *data.as_ptr().cast::<SERVICE_FAILURE_ACTIONSW>() };
        let mut empty = SC_ACTION::default();
        if actions.cActions == 0 {
            actions.lpsaActions = &mut empty;
        }
        // SAFETY: a non-null empty array explicitly clears actions; other pointers refer to the captured buffer.
        return checked(unsafe {
            ChangeServiceConfig2W(
                service.0,
                kind,
                (&actions as *const SERVICE_FAILURE_ACTIONSW).cast(),
            )
        });
    }
    // SAFETY: the aligned captured buffer contains the matching queried structure and live embedded pointers.
    checked(unsafe { ChangeServiceConfig2W(service.0, kind, data.as_ptr().cast()) })
}

fn query(mut call: impl FnMut(*mut u8, u32, *mut u32) -> i32) -> io::Result<Vec<usize>> {
    let mut needed = 0;
    let ok = call(ptr::null_mut(), 0, &mut needed);
    let error = io::Error::last_os_error();
    if ok == 0 && error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(error);
    }
    if needed == 0 {
        return Err(io::Error::other(
            "service configuration query returned no data",
        ));
    }
    let mut data = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    checked(call(data.as_mut_ptr().cast(), needed, &mut needed))?;
    Ok(data)
}
