# quickbench

A tiny no-frills micro-benchmark harness: a `Bencher` builder plus a
`#[quick_bench]` attribute macro that expands to an `#[ignore]`d `#[test]`.
There is no separate `cargo bench` target — benchmarks run through
`cargo test -- --ignored`, and debug-mode runs print a warning because the
numbers are meaningless.

Benchmark runs are serialized across processes, and each result is written to a
file so the next run can print a coloured faster/slower comparison against it.

The public surface is intentionally tiny; keep the supporting machinery
private.
