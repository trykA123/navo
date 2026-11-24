//! The IW8a refused folders: known folders Setup never installs into, inside or onto.
//! A child of `chain.rs`, which resolves each folder to its file-id anchor.

use std::io;

use navo_core::brand;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData,
    FOLDERID_LocalAppDataLow, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesCommon,
    FOLDERID_ProgramFilesCommonX86, FOLDERID_ProgramFilesX86, FOLDERID_Public,
    FOLDERID_RoamingAppData, FOLDERID_SkyDrive, FOLDERID_StartMenu, FOLDERID_Startup,
    FOLDERID_System, FOLDERID_UserProfiles, FOLDERID_UserProgramFiles, FOLDERID_Videos,
    FOLDERID_Windows,
};

use super::{add_optional, add_optional_unverified, add_required};
use crate::placement::{Protected, Refused};

/// Velopack's pack id, which is also the legacy root folder's name.
const NAVO_APP_ROOT: &str = "NavoApp";

/// Builds the IW8a and IW28 refused folders; an absent one anchors at its nearest existing parent.
/// The Windows, Program Files, ProgramData, profile and app-data folders are mandatory: a failed
/// lookup fails. Only the `add_optional` folders may skip an `E_FAIL` or not-found lookup.
pub fn refused() -> io::Result<Refused> {
    let mut never_inside = Vec::new();
    add_required(&mut never_inside, &FOLDERID_Windows, &[])?;
    add_required(&mut never_inside, &FOLDERID_System, &[])?;
    add_required(&mut never_inside, &FOLDERID_ProgramFiles, &[])?;
    add_optional(&mut never_inside, &FOLDERID_ProgramFilesX86, &[])?;
    add_optional(&mut never_inside, &FOLDERID_ProgramFilesCommon, &[])?;
    add_optional(&mut never_inside, &FOLDERID_ProgramFilesCommonX86, &[])?;
    add_required(&mut never_inside, &FOLDERID_ProgramFiles, &["WindowsApps"])?;
    add_required(&mut never_inside, &FOLDERID_ProgramData, &[])?;
    add_required(&mut never_inside, &FOLDERID_LocalAppData, &[brand::NAME])?;
    add_required(&mut never_inside, &FOLDERID_RoamingAppData, &[brand::NAME])?;
    add_required(
        &mut never_inside,
        &FOLDERID_LocalAppData,
        &[brand::LEGACY_NAME],
    )?;
    add_required(
        &mut never_inside,
        &FOLDERID_RoamingAppData,
        &[brand::LEGACY_NAME],
    )?;
    add_optional(&mut never_inside, &FOLDERID_SkyDrive, &[])?;
    add_optional(&mut never_inside, &FOLDERID_Startup, &[])?;

    let mut never_equal = Vec::new();
    add_required(&mut never_equal, &FOLDERID_Profile, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Desktop, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Documents, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Downloads, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Pictures, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Music, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Videos, &[])?;
    add_required(&mut never_equal, &FOLDERID_LocalAppData, &[])?;
    add_required(&mut never_equal, &FOLDERID_RoamingAppData, &[])?;
    add_optional(&mut never_equal, &FOLDERID_LocalAppDataLow, &[])?;
    add_optional_unverified(&mut never_equal, &FOLDERID_UserProgramFiles)?;
    add_optional(&mut never_equal, &FOLDERID_StartMenu, &[])?;
    add_optional(&mut never_equal, &FOLDERID_Public, &[])?;

    let mut fresh_only = Vec::new();
    add_required(&mut fresh_only, &FOLDERID_LocalAppData, &[NAVO_APP_ROOT])?;

    // Atom 0: the same two folders also refuse with NeedsAdministrator; E10 removes this group.
    let mut admin_only = Vec::new();
    add_required(&mut admin_only, &FOLDERID_ProgramFiles, &[])?;
    add_optional(&mut admin_only, &FOLDERID_ProgramFilesX86, &[])?;

    Ok(Refused {
        never_inside,
        never_equal,
        fresh_only,
        admin_only,
        service: service()?,
        profiles: profiles()?,
    })
}

/// The IW28 index service folders, under the current and the legacy brand name.
fn service() -> io::Result<Vec<Protected>> {
    let mut service = Vec::new();
    add_required(&mut service, &FOLDERID_ProgramFiles, &[brand::NAME])?;
    add_required(&mut service, &FOLDERID_ProgramFiles, &[brand::LEGACY_NAME])?;
    Ok(service)
}

/// The IW28 parent of every user's profile, usually `C:\Users`.
fn profiles() -> io::Result<Vec<Protected>> {
    let mut profiles = Vec::new();
    add_required(&mut profiles, &FOLDERID_UserProfiles, &[])?;
    Ok(profiles)
}
