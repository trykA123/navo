//! The IW8a refused folders: known folders Setup never installs into, inside or onto.
//! A child of `chain.rs`, which resolves each folder to its file-id anchor.

use std::io;

use navo_core::brand;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData,
    FOLDERID_LocalAppDataLow, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesCommon,
    FOLDERID_ProgramFilesX86, FOLDERID_Public, FOLDERID_RoamingAppData, FOLDERID_SkyDrive,
    FOLDERID_StartMenu, FOLDERID_Startup, FOLDERID_System, FOLDERID_UserProgramFiles,
    FOLDERID_Videos, FOLDERID_Windows,
};

use super::{add_known, add_known_unverified};
use crate::placement::Refused;

/// Velopack's pack id, which is also the legacy root folder's name.
const NAVO_APP_ROOT: &str = "NavoApp";

/// Builds the IW8a refused folders, anchoring absent folders at their nearest existing parent.
pub fn refused() -> io::Result<Refused> {
    let mut never_inside = Vec::new();
    add_known(&mut never_inside, &FOLDERID_Windows, &[])?;
    add_known(&mut never_inside, &FOLDERID_System, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramFiles, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramFilesX86, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramFilesCommon, &[])?;
    add_known(&mut never_inside, &FOLDERID_ProgramData, &[])?;
    add_known(&mut never_inside, &FOLDERID_LocalAppData, &[brand::NAME])?;
    add_known(&mut never_inside, &FOLDERID_RoamingAppData, &[brand::NAME])?;
    add_known(
        &mut never_inside,
        &FOLDERID_LocalAppData,
        &[brand::LEGACY_NAME],
    )?;
    add_known(
        &mut never_inside,
        &FOLDERID_RoamingAppData,
        &[brand::LEGACY_NAME],
    )?;
    add_known(&mut never_inside, &FOLDERID_SkyDrive, &[])?;
    add_known(&mut never_inside, &FOLDERID_Startup, &[])?;

    let mut never_equal = Vec::new();
    add_known(&mut never_equal, &FOLDERID_Profile, &[])?;
    add_known(&mut never_equal, &FOLDERID_Desktop, &[])?;
    add_known(&mut never_equal, &FOLDERID_Documents, &[])?;
    add_known(&mut never_equal, &FOLDERID_Downloads, &[])?;
    add_known(&mut never_equal, &FOLDERID_Pictures, &[])?;
    add_known(&mut never_equal, &FOLDERID_Music, &[])?;
    add_known(&mut never_equal, &FOLDERID_Videos, &[])?;
    add_known(&mut never_equal, &FOLDERID_LocalAppData, &[])?;
    add_known(&mut never_equal, &FOLDERID_RoamingAppData, &[])?;
    add_known(&mut never_equal, &FOLDERID_LocalAppDataLow, &[])?;
    add_known_unverified(&mut never_equal, &FOLDERID_UserProgramFiles)?;
    add_known(&mut never_equal, &FOLDERID_StartMenu, &[])?;
    add_known(&mut never_equal, &FOLDERID_Public, &[])?;

    let mut fresh_only = Vec::new();
    add_known(&mut fresh_only, &FOLDERID_LocalAppData, &[NAVO_APP_ROOT])?;

    // Atom 0: the same two folders also refuse with NeedsAdministrator; E10 removes this group.
    let mut admin_only = Vec::new();
    add_known(&mut admin_only, &FOLDERID_ProgramFiles, &[])?;
    add_known(&mut admin_only, &FOLDERID_ProgramFilesX86, &[])?;

    Ok(Refused {
        never_inside,
        never_equal,
        fresh_only,
        admin_only,
    })
}
