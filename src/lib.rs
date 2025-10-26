#![feature(likely_unlikely)]
mod raw_cell;

#[cfg(test)]
#[cfg(feature = "test-crate")]
mod tests;

pub use raw_cell::RawCell;