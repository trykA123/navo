#[cfg(test)]
mod fixture;

#[cfg(test)]
#[path = "cli_remote/support.rs"]
mod cli_remote_support;

#[cfg(test)]
#[cfg(unix)]
#[path = "cli_remote/origin.rs"]
mod cli_remote_origin;

#[cfg(test)]
#[cfg(target_os = "linux")]
#[path = "cli_remote/challenge.rs"]
mod cli_remote_challenge;

#[cfg(test)]
#[path = "cli_remote/fetch.rs"]
mod cli_remote_fetch;

#[cfg(test)]
#[path = "cli_remote/refuse.rs"]
mod cli_remote_refuse;

#[cfg(test)]
#[cfg(unix)]
#[path = "cli_remote/safety.rs"]
mod cli_remote_safety;
