#[cfg(test)]
mod fixture;

#[cfg(test)]
#[path = "trust/support.rs"]
mod trust_support;

#[cfg(test)]
#[path = "trust/config.rs"]
mod trust_config;

#[cfg(test)]
#[path = "trust/hostile.rs"]
mod trust_hostile;

#[cfg(test)]
#[path = "trust/git.rs"]
mod trust_git;

#[cfg(test)]
#[path = "trust/escape.rs"]
mod trust_escape;

#[cfg(test)]
#[path = "trust/fetch.rs"]
mod trust_fetch;

#[cfg(test)]
#[path = "trust/resolve.rs"]
mod trust_resolve;
