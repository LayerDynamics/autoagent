//! Node.js backends. Exactly one compiles per build, selected by a Cargo
//! feature. Both adapter files are generated from the surface registry.

#[cfg(feature = "node-napi")]
pub mod napi;

#[cfg(feature = "node-bindgen")]
pub mod node_bindgen;
