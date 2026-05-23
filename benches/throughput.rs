//! Criterion benches for `rusty-vipe`. Gated behind the `bench` feature.
//!
//! **STUB** — full criterion harness lands in Polish (T120/T121/T122).

#[cfg(feature = "bench")]
fn main() {
    eprintln!("rusty-vipe: bench harness not yet implemented (Polish phase)");
}

#[cfg(not(feature = "bench"))]
fn main() {
    eprintln!("rusty-vipe: rebuild with --features bench to run throughput benches");
}
