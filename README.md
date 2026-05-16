# quickbench

A tiny, no-frills micro-benchmark harness for Rust. Benchmarks are plain
`#[test]` functions (gated with `#[ignore]`), so they live alongside your code
and run through `cargo test` — no separate `cargo bench` target, no nightly,
no Criterion-sized dependency tree.

- Warmup + measurement phases, each bounded by time, iterations, or both
- Per-bench result files with automatic comparison against the previous run
  (coloured `faster` / `SLOWER` diff)
- Cross-process serialization via a named lock — parallel `cargo test`
  invocations on the same machine won't trample each other
- Labeled sub-benches for comparing variants in one run

## Quick start

```toml
[dev-dependencies]
quickbench = { path = "path/to/quickbench" }
```

```rust
use quickbench::{Bencher, quick_bench};

#[quick_bench(warmup_time_ms = 100, bench_time_ms = 500)]
fn bench_vec_sum(b: Bencher) {
    b.bench(|| {
        let v: Vec<u64> = (0..1_000).collect();
        v.iter().sum::<u64>()
    });
}

#[quick_bench(warmup_time_ms = 100, bench_time_ms = 500)]
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
```

Run:

```bash
cargo test --release -- --ignored --nocapture
```

Output:

```
[BENCH] bench_sort_variants/stable:
10.625µs (min: 9.917µs, max: 69.084µs, median: 10.5µs, 93185 iters)

[BENCH] bench_sort_variants/unstable:
9.083µs (min: 8.875µs, max: 31.250µs, median: 9.000µs, 110044 iters)
   ↳ 14.3% faster than previous run
```

Results are persisted to `bench-results/<bench_name>.txt` in the consuming
crate. The next run compares against that file and prints a diff (threshold
±5%).

## The `#[quick_bench]` macro

`#[quick_bench]` expands a function into a `#[test] #[ignore]` that
constructs a `Bencher`, hands it to the body, and writes the result. The
attributes are inert outside the test harness, so you can still call the
function from `main` (see `examples/macro_example.rs`).

Supported arguments:

| arg | meaning |
|---|---|
| `warmup_time_ms = N` | warmup duration cap (default 1000) |
| `warmup_iters = N` | warmup iteration cap |
| `bench_time_ms = N` | measurement duration cap (default 5000) |
| `iters = N` | measurement iteration cap |
| `ignore = false` | drop the `#[ignore]` (runs on plain `cargo test`) |

Whichever stop condition fires first wins. At least one per phase must be
set.

## Zed integration

Because benches are `#[test] #[ignore]` and need `--release` to be meaningful,
the inline Zed "Run" buttons need a small nudge. Drop this in
`.zed/tasks.json`:

```json
[
  {
    "label": "cargo run --release --example $ZED_STEM",
    "command": "cargo",
    "args": ["run", "--release", "--example", "$ZED_STEM"],
    "tags": ["rust-main"]
  },
  {
    "label": "cargo test --release $ZED_SYMBOL",
    "command": "cargo",
    "args": [
      "test",
      "-p", "$ZED_CUSTOM_RUST_PACKAGE",
      "--release",
      "$ZED_SYMBOL",
      "--",
      "--ignored",
      "--nocapture",
      "--show-output"
    ],
    "tags": ["rust-test"]
  }
]
```

The `rust-test` task binds to any `#[test]` (including macro-expanded
`#[quick_bench]`), runs it in release with `--ignored`, and scopes the build
to the current workspace member. The `rust-main` task does the same for
`examples/*.rs` so you can hit Run on an example file and not silently get a
debug build.

## License

Dual-licensed under either of

- [MIT license](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.
