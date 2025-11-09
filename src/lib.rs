#![feature(likely_unlikely)]
#![feature(const_trait_impl)]
pub mod utils;
pub mod raw_cell;
pub mod traits;

#[cfg(test)]
#[cfg(feature = "test-crate")]
mod tests;

pub use raw_cell::RawCell;