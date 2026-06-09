//! Python backends. Exactly one compiles per build, selected by a Cargo
//! feature. Both adapter files are generated from the surface registry.

#[cfg(feature = "py-pyo3")]
pub mod pyrs;

#[cfg(feature = "py-rustpython")]
pub mod python_bingen;
