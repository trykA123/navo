//! Profile of Sidewise: where its files live, which format version they have,
//! and the store of saved sessions. Pure logic with no GUI.

pub mod appearance;
pub mod book;
pub mod brand;
pub mod paths;
pub mod pick;
pub mod recent;
pub mod session;
pub mod session_store;
pub mod update_state;

#[cfg(test)]
mod appearance_tests;

#[cfg(test)]
mod appearance_layout_tests;

#[cfg(test)]
mod book_tests;

#[cfg(test)]
mod paths_tests;

#[cfg(test)]
mod pick_tests;

#[cfg(test)]
mod recent_tests;

#[cfg(test)]
mod remote_side_tests;

#[cfg(test)]
mod session_tests;

#[cfg(test)]
mod session_store_tests;
