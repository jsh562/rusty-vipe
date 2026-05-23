//! Shared test harness helpers for integration tests.

#![allow(dead_code)]

use std::collections::HashSet;
use std::path::PathBuf;

/// Locate the `fake-editor` helper binary built by Cargo with the
/// `dev-helpers` feature. `CARGO_BIN_EXE_<name>` is set by Cargo at test
/// build time per https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates.
pub fn fake_editor_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-editor"))
}

/// Allocate a fresh tempdir scoped to the test.
pub fn with_tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("could not create test tempdir")
}

/// Snapshot the names of every entry currently in `dir`. Used by FR-032
/// before/after assertions to confirm no `.rusty-vipe-*` or `tmp-*` tempfile
/// survived a given run.
pub fn snapshot_tempdir(dir: &std::path::Path) -> HashSet<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    read.filter_map(|e| e.ok()).map(|e| e.path()).collect()
}

/// Assert that no `.rusty-vipe-*` artifact remains in the diff between two
/// snapshots taken before and after a vipe run.
pub fn assert_no_rusty_vipe_artifacts(before: &HashSet<PathBuf>, after: &HashSet<PathBuf>) {
    let leaked: Vec<_> = after
        .difference(before)
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".rusty-vipe-") || n.starts_with("tmp-"))
        })
        .collect();
    assert!(
        leaked.is_empty(),
        "rusty-vipe artifact(s) leaked into tempdir: {leaked:?}"
    );
}
