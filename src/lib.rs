//! Simple benchmarking utilities for use in tests.
//!
//! This crate provides a lightweight benchmarking framework that runs as regular tests
//! but measures execution time with proper warmup and statistics.
//!
//! ## Cross-Process Serialization
//!
//! Benchmarks are serialized across processes using a named system lock. By default the
//! lock name is scoped per-crate (the `#[quick_bench]` macro passes `CARGO_PKG_NAME`), so
//! unrelated projects don't contend with each other. Override with [`Bencher::with_lock_name`]
//! or disable with [`Bencher::without_lock`].
//!
//! ## Result files
//!
//! A plain [`Bencher`] writes no files. [`Bencher::with_output_dir`] opts in, and so does
//! `#[quick_bench]`, which passes the crate's manifest directory. The `QUICKBENCH_OUTPUT_DIR`
//! environment variable, set at run time, takes precedence over either. Results land in
//! `<dir>/bench-results/<name>.txt`.

mod bench_result;
mod bencher;
mod colors;
mod comparison;

pub use bench_result::BenchResult;
pub use bencher::Bencher;
pub use quickbench_macros::quick_bench;
