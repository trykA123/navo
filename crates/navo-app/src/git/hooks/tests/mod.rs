use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navo_fs::DriveClass;
use navo_git::status::{RepoStatus, parse_porcelain_v2};
use navo_store::GitEngine;
use navo_ui::git::{BadgeKind, BadgeReason};

use std::path::{Path, PathBuf};

use navo_core::Location;
use navo_ui::git::{GitAction, GitHooks, GitMenuItem, GitStarted, Previewed, Shown};

use navo_git::{Cancel as RunCancel, SyncKind, SyncSummary};

use super::super::launch::start::Ports as LaunchPorts;
use super::super::launch::{Launch, LaunchError};
use super::super::pullpush::tests::{confirmation, confirmation_of};
use super::super::pullpush::{Confirmation, SyncFailure, SyncPort};
use super::super::scan::{Cancel, Ports, Probe, ScanError, StatusSource};
use super::super::tools::{DetectedTool, ToolKind};
use super::super::wiring::{ToolPort, Tools};
use super::*;

mod live;
mod previews;
mod rig;
mod run;
mod scanner;
mod tools;
mod wired;

use rig::*;
use wired::*;
