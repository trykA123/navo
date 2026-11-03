#[cfg(test)]
mod fixture;

#[cfg(test)]
#[path = "gix_status/support.rs"]
mod gix_status_support;

#[cfg(test)]
#[path = "gix_status/oracle.rs"]
mod gix_status_oracle;

#[cfg(test)]
#[path = "gix_status/limits.rs"]
mod gix_status_limits;

#[cfg(test)]
#[path = "gix_status/safety.rs"]
mod gix_status_safety;

#[cfg(test)]
#[path = "gix_status/measure.rs"]
mod gix_status_measure;
