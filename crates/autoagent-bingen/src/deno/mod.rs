//! Deno backends. Both load a C-ABI `cdylib` via `Deno.dlopen`; the
//! `deno_bindgen` path also generates a typed `mod.ts` wrapper, while `ffi`
//! is the dependency-free raw path. Adapter files are generated.

#[cfg(feature = "deno-bindgen")]
pub mod deno_bindgen;

#[cfg(feature = "deno-ffi")]
pub mod ffi;
