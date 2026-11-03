#[cfg(test)]
mod fixture;

#[cfg(test)]
#[path = "cli_remote/support.rs"]
mod cli_remote_support;

#[cfg(test)]
#[path = "cli_remote/origin.rs"]
mod cli_remote_origin;

#[cfg(test)]
#[cfg(target_os = "linux")]
#[path = "cli_remote/challenge.rs"]
mod cli_remote_challenge;

#[cfg(test)]
#[path = "cli_sync/support.rs"]
mod cli_sync_support;

#[cfg(test)]
#[path = "cli_sync/pull.rs"]
mod cli_pull;

#[cfg(test)]
#[path = "cli_sync/pull_refuse.rs"]
mod cli_pull_refuse;

#[cfg(test)]
#[path = "cli_sync/pull_config.rs"]
mod cli_pull_config;

#[cfg(test)]
#[path = "cli_sync/push.rs"]
mod cli_push;

#[cfg(test)]
#[path = "cli_sync/push_safety.rs"]
mod cli_push_safety;

#[cfg(test)]
#[cfg(unix)]
#[path = "cli_sync/push_safety_log.rs"]
mod cli_push_safety_log;

#[cfg(test)]
#[path = "cli_sync/pull_kill.rs"]
mod cli_pull_kill;

#[cfg(test)]
#[path = "cli_sync/bound.rs"]
mod cli_pull_push_bound;

#[cfg(test)]
#[path = "cli_sync/pull_race.rs"]
mod cli_pull_race;
