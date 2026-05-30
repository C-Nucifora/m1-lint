//! Corpus smoke test — skipped unless `M1_CORPUS_PATH` is set.
//!
//! Run with:
//!   M1_CORPUS_PATH=../m1-example/UQR-EV/01.00/Scripts cargo test --test corpus

use m1_lint::registry::Registry;
use m1_lint::runner::Runner;
use std::path::Path;

#[test]
fn corpus_no_panic() {
    let corpus_path = match std::env::var("M1_CORPUS_PATH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("M1_CORPUS_PATH not set — skipping corpus test");
            return;
        }
    };

    let dir = Path::new(&corpus_path);
    assert!(
        dir.is_dir(),
        "M1_CORPUS_PATH is not a directory: {}",
        dir.display()
    );

    let runner = Runner::new(Registry::default_v1());
    let mut count = 0usize;

    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("m1scr") {
            continue;
        }
        let result = runner
            .run_file(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        // No assertion on diagnostic counts — just assert no panic.
        drop(result);
        count += 1;
    }

    assert!(count > 0, "no .m1scr files found in corpus");
    eprintln!("corpus smoke test passed: {} files checked", count);
}
