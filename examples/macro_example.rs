//! Example use of the `#[quick_bench]` macro.
//!
//! `#[quick_bench]` expands to `#[test] #[ignore]`. Outside the test harness the
//! attributes are inert, so the generated functions can simply be called from `main`.
//!
//! Run with:
//!   cargo run --release --example macro_example

use quickbench::{Bencher, quick_bench};

#[quick_bench(warmup_time_ms = 100, bench_time_ms = 300)]
fn bench_vec_sum(b: Bencher) {
    b.bench(|| {
        let v: Vec<u64> = (0..1_000).collect();
        v.iter().sum::<u64>()
    });
}

#[quick_bench(warmup_iters = 100, iters = 5_000)]
fn bench_string_build(b: Bencher) {
    b.bench(|| {
        let mut s = String::new();
        for i in 0..100 {
            s.push_str(&i.to_string());
        }
        s
    });
}

#[quick_bench(warmup_time_ms = 100, bench_time_ms = 300)]
fn bench_sort_variants(b: Bencher) {
    let input: Vec<i32> = (0..1_000).rev().collect();

    b.bench_labeled("stable", || {
        let mut v = input.clone();
        v.sort();
        v
    });

    b.bench_labeled("unstable", || {
        let mut v = input.clone();
        v.sort_unstable();
        v
    });
}

fn main() {
    bench_vec_sum();
    bench_string_build();
    bench_sort_variants();
}
