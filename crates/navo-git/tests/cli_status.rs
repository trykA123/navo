#[cfg(test)]
mod fixture;

#[cfg(test)]
#[path = "cli_status/support.rs"]
mod cli_status_support;

#[cfg(test)]
#[path = "cli_status/states.rs"]
mod cli_status_states;

#[cfg(test)]
#[path = "cli_status/limits.rs"]
mod cli_status_limits;

#[cfg(test)]
#[path = "cli_status/safety.rs"]
mod cli_status_safety;
