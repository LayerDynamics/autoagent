//! Build orchestration: configure the napi build when that backend is selected,
//! and rebuild whenever the surface registry changes.

fn main() {
    #[cfg(feature = "node-napi")]
    napi_build::setup();

    println!("cargo:rerun-if-changed=bind.rs");
}
