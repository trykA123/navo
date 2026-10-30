//! Single running copy: the first Navo serves requests, a second one hands its request over and exits.

use std::ops::ControlFlow;
use std::process::ExitCode;

use rusty_ui::window_view::OpenSender;

use crate::cli::Start;
use crate::profile::Profile;
use crate::{console, gui, instance, metrics, splash};

pub(crate) type First = (Option<instance::Guard>, splash::Splash);

pub(crate) fn first(guard: Option<instance::Guard>) -> ControlFlow<ExitCode, First> {
    let splash = splash::Splash::start();
    metrics::mark("splash started");
    ControlFlow::Continue((guard, splash))
}

pub(crate) fn prepare_instance(
    start: &Start,
    profile: &Profile,
    sender: OpenSender,
) -> Result<ControlFlow<ExitCode, First>, instance::Error> {
    prepare_instance_with_notice(start, profile, sender, console::show_mode_mismatch)
}

pub(crate) fn prepare_instance_with_notice(
    start: &Start,
    profile: &Profile,
    sender: OpenSender,
    show_notice: impl FnOnce(&str),
) -> Result<ControlFlow<ExitCode, First>, instance::Error> {
    if start.is_metrics() {
        return Ok(ControlFlow::Continue((None, splash::Splash::none())));
    }
    let Some(dirs) = profile.dirs() else {
        return Ok(first(None));
    };
    let Some(acquired) = instance::acquire(&dirs.data)? else {
        return Ok(first(None));
    };
    match acquired {
        instance::Acquired::First(mut guard) => {
            let retention = start.retention;
            guard.serve(move |request| {
                if request.experimental_retention != retention {
                    return instance::Reply::Error;
                }
                gui::on_instance_request(&sender, &request)
            })?;
            Ok(first(Some(guard)))
        }
        instance::Acquired::Second(client) => {
            if start.trace.is_some() {
                console::attach();
                eprintln!("navo: close the running Navo before starting --trace");
                return Ok(ControlFlow::Break(ExitCode::FAILURE));
            }
            if start.legacy_retention_flag {
                let _diagnostics = crate::install_logging(profile);
                start.log_deprecated_flags();
            }
            let request = instance::Request::from_start(start, &std::env::current_dir()?)?;
            let reply = client.send(&request, std::time::Duration::from_secs(2))?;
            let code = if reply == instance::Reply::Ok {
                ExitCode::SUCCESS
            } else {
                console::attach();
                eprintln!("navo: the running Navo rejected the request");
                show_notice(&rusty_l10n::tr("instance-mode-mismatch"));
                ExitCode::FAILURE
            };
            Ok(ControlFlow::Break(code))
        }
    }
}
