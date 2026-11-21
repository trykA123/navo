//! Screens that the shell's tabs show: the compare run model today, the compare tab later.
//! Each screen owns its state and reports changes through the shell.

pub mod compare_host;
#[cfg(test)]
mod compare_host_tests;
mod compare_labels;
pub mod compare_roots;
#[cfg(test)]
mod compare_roots_tests;
pub mod compare_run;
#[cfg(test)]
mod compare_run_phase_tests;
pub mod compare_tab;
mod compare_tab_events;
pub mod compare_tab_keys;
mod compare_tab_large;
pub mod folders_cells;
#[cfg(test)]
mod folders_cells_tests;
mod folders_cells_view;
pub mod folders_head;
pub mod folders_model;
#[cfg(test)]
mod folders_model_tests;
mod folders_row;
pub mod folders_text;
#[cfg(test)]
mod folders_text_tests;
pub mod folders_view;
pub(crate) mod git_remote_sides;
pub(crate) mod git_sides;
pub(crate) mod host_parts;
pub mod large_model;
#[cfg(test)]
mod large_model_tests;
pub mod large_view;
#[cfg(test)]
mod large_view_tests;
pub mod pair_requests;
#[cfg(test)]
mod pair_requests_tests;
pub mod reveal;
