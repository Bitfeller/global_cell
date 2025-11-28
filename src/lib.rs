#![feature(likely_unlikely)]
#![feature(const_trait_impl)]
pub mod utils;
pub mod traits;
pub mod raw_cell;
pub mod cell;

#[cfg(test)]
#[cfg(feature = "test-crate")]
mod tests;

pub use crate::cell::Cell;