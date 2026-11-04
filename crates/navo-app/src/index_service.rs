//! App-side facade over the index service library.

pub(crate) use navo_index_service::{Action, run};

#[cfg(any(windows, test))]
pub(crate) use crate::service_version_step::is_older;
#[cfg(windows)]
pub(crate) use crate::service_version_step::start_version_step;
#[cfg(any(windows, test))]
pub(crate) use navo_index_service::{ElevationError, ServiceQuery};
#[cfg(test)]
pub(crate) use navo_index_service::{ServiceDrive, selected_roots};
#[cfg(windows)]
pub(crate) use navo_index_service::{
    dismiss_upgrade_notice, installed_volume_roots, query, request_elevation, upgrade_notice_due,
};
