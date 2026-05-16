//! Compare two sort implementations using `Bencher::bench_labeled`.
//!
//! Run with:
//!   cargo run --release --example sort_compare
//!
//! To persist results for diff-vs-previous, either chain `.with_output_dir("...")`
//! or set `QUICKBENCH_OUTPUT_DIR=./target` before running.

use quickbench::Bencher;

fn make_input() -> Vec<i32> {
    (0..1_000).rev().collect()
}

fn main() {
    let b = Bencher::new("sort_1k_reversed")
        .with_warmup_time_ms(200)
        .with_bench_time_ms(1_000);

    b.bench_labeled("std_sort", || {
        let mut v = make_input();
        v.sort();
        v
    });

    b.bench_labeled("std_sort_unstable", || {
        let mut v = make_input();
        v.sort_unstable();
        v
    });
}
