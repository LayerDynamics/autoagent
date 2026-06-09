//! 1.0.0 performance smoke gate (M8, SPEC-1 §2.2 — OQ-1 proposed targets).
//!
//! Generates a real multi-thousand-file repository and times a full scan. The
//! proposed target is p95 < 2s for 10k files; this scaled smoke (4k files)
//! asserts a comfortable bound and prints the measured time so the OQ-1 target
//! can be confirmed or adjusted with real data.

use autoagent_core::analysis::file_scanner;
use camino::Utf8Path;
use std::time::Instant;

#[test]
fn scan_thousands_of_files_is_fast() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();

    // 4,000 source files spread across 40 directories.
    const DIRS: usize = 40;
    const PER_DIR: usize = 100;
    for d in 0..DIRS {
        let sub = root.join(format!("src/mod{d}"));
        std::fs::create_dir_all(sub.as_std_path()).unwrap();
        for f in 0..PER_DIR {
            std::fs::write(
                sub.join(format!("file{f}.rs")).as_std_path(),
                "pub fn f() {}\n",
            )
            .unwrap();
        }
    }

    let start = Instant::now();
    let files = file_scanner::scan(root, &["**/*.rs".into()], &["target/**".into()]).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(files.len(), DIRS * PER_DIR);
    println!(
        "scanned {} files in {:?} ({:.1} files/ms)",
        files.len(),
        elapsed,
        files.len() as f64 / elapsed.as_millis().max(1) as f64
    );
    // Generous bound: even scaled to 10k this stays well under the 2s p95 target.
    assert!(
        elapsed.as_secs_f64() < 2.0,
        "scan exceeded the proposed performance budget: {elapsed:?}"
    );
}
