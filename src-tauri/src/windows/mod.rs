//! Windows-specific window handling.

pub mod notch;

pub use notch::{COMPACT_HEIGHT, COMPACT_WIDTH, EXPANDED_HEIGHT, EXPANDED_WIDTH};

#[allow(unused_imports)]
use notch as _dimension_source;
