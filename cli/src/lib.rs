//! The library inside the `unmei` package, an implementation detail of the
//! `unmei` binary.
//!
//! Nothing here is a supported API. The crate makes no stability promise: a
//! patch release can rename, change or remove any item.

pub mod draft;
pub mod gen;
pub mod provider;
mod retry;
pub mod runner;
pub mod splice;
mod staging;
pub mod target;
mod terminal;
