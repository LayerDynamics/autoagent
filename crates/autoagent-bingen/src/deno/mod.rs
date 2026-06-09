//! Deno backends. Both load a C-ABI `cdylib` via `Deno.dlopen`. `deno_bindgen`
//! is the primary, annotation-based path (its CLI generates the typed bindings);
//! `ffi` is the dependency-free raw path consumed by the generated `deno/mod.ts`.
//! Both adapter files are generated from the surface registry.

#[cfg(feature = "deno-bindgen")]
pub mod deno_bindgen;

#[cfg(feature = "deno-ffi")]
pub mod ffi;
