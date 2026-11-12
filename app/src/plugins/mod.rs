//! Plugin composition: [`crate::RuntimeCorePlugin`]`'s headless-capable core
//! frame, [`crate::UnmeiPlugin`]`'s shipping surface, and the ownership-gated
//! presentation registration they share with the input-ownership harness.

mod core;
mod shipping;

#[cfg(test)]
pub(crate) use shipping::register_ownership_gated_presentation;
