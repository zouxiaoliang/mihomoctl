#![doc = include_str!("../README.md")]

mod_use::mod_use![api, error];

#[cfg(test)]
mod test;

pub mod model;

pub use serde_json;
#[cfg(feature = "enum_ext")]
pub use strum;
