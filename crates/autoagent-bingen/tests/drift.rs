//! Drift guard (FR-15 / R-2): the generated files committed to the repo must be
//! byte-identical to a fresh regeneration from `bind.rs`. If this fails, run
//! `cargo run -p autoagent-bingen -- generate` and commit the result.

use autoagent_bingen::gen;

#[test]
fn generated_files_match_committed_golden() {
    gen::check().expect("generated files are out of date — run `bingen generate`");
}
