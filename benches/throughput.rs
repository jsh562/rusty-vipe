//! Criterion benches for `rusty-vipe`. Gated behind the `bench` feature.
//!
//! Three groups per plan §Testing Strategy + HINT-005:
//!
//! 1. **cold-start `--version`** — process launch + clap version path; bench
//!    target per plan Performance Goals: p95 ≤ 50 ms on the reference runner.
//! 2. **editor-spawn-and-return** — full drain → spawn → write-back loop
//!    against `fake-editor --transform=noop` so the bench reflects only our
//!    pipeline overhead, not the editor's execution time (HINT-005).
//! 3. **tempfile-setup** — `pipeline::drain_to_tempfile` cost alone, isolating
//!    the per-invocation filesystem allocation from process-spawn overhead.
//!
//! Run with: `cargo bench --features bench,cli,dev-helpers`.

#![cfg(feature = "bench")]

use std::io::Cursor;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// Path to the built `rusty-vipe` binary. `CARGO_BIN_EXE_<name>` is set by
/// Cargo at bench-build time.
fn rusty_vipe_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rusty-vipe"))
}

/// Path to the built `fake-editor` helper.
fn fake_editor_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-editor"))
}

/// `EDITOR` value pointing at the fake-editor with the given transform —
/// matches the form used by integration tests (POSIX-single-quoted so
/// shell-words preserves the transform value literally on every platform).
fn editor_env_value(transform: &str) -> String {
    let path = fake_editor_bin().to_string_lossy().replace('\\', "/");
    format!("{path} '--transform={transform}'")
}

fn bench_cold_start_version(c: &mut Criterion) {
    let bin = rusty_vipe_bin();
    let mut group = c.benchmark_group("cold-start");
    group.sample_size(100);
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.bench_function("version", |b| {
        b.iter(|| {
            let status = Command::new(&bin)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("spawn rusty-vipe --version");
            assert!(status.success());
        })
    });
    group.finish();
}

fn bench_editor_spawn_and_return(c: &mut Criterion) {
    let bin = rusty_vipe_bin();
    let editor = editor_env_value("noop");
    let mut group = c.benchmark_group("editor-spawn-and-return");
    group.sample_size(60);
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.throughput(Throughput::Elements(1));
    group.bench_function("noop", |b| {
        b.iter(|| {
            let mut child = Command::new(&bin)
                .env("RUSTY_VIPE_TEST_BYPASS_TTY", "1")
                .env("EDITOR", &editor)
                .env_remove("VISUAL")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn rusty-vipe");
            // Empty stdin: drain → spawn → write-back roundtrips with no edit work.
            drop(child.stdin.take());
            let status = child.wait().expect("wait rusty-vipe");
            assert!(status.success(), "noop bench should succeed");
        })
    });
    group.finish();
}

fn bench_tempfile_setup(c: &mut Criterion) {
    let mut group = c.benchmark_group("tempfile-setup");
    group.sample_size(200);
    group.warm_up_time(std::time::Duration::from_millis(500));

    // Small input — exercises the allocate-and-write path with negligible IO.
    let small_payload: Vec<u8> = b"alpha\nbravo\ncharlie\n".to_vec();
    group.bench_function("small-19-bytes", |b| {
        b.iter(|| {
            let reader = Cursor::new(&small_payload);
            let tf =
                rusty_vipe::pipeline::drain_to_tempfile(reader, ".txt").expect("drain succeeds");
            // Drop runs the cleanup path — include it in the timing so the
            // bench reflects per-invocation cost end-to-end.
            drop(tf);
        })
    });

    // 64 KiB synthetic payload — exercises the actual write loop, not just allocation.
    let medium_payload: Vec<u8> = vec![b'x'; 64 * 1024];
    group.throughput(Throughput::Bytes(medium_payload.len() as u64));
    group.bench_function("64-kib", |b| {
        b.iter(|| {
            let reader = Cursor::new(&medium_payload);
            let tf =
                rusty_vipe::pipeline::drain_to_tempfile(reader, ".txt").expect("drain succeeds");
            drop(tf);
        })
    });

    group.finish();
}

criterion_group!(
    name = throughput;
    config = Criterion::default();
    targets = bench_cold_start_version, bench_editor_spawn_and_return, bench_tempfile_setup
);
criterion_main!(throughput);
