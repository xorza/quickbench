# quickbench

A micro-benchmark harness: a `Bencher` builder plus `#[quick_bench]`, which
expands to an `#[ignore]`d `#[test]`. There is no `cargo bench` target —
benchmarks run through `cargo test --release -- --ignored`; a debug build
prints a warning because its numbers are meaningless. Runs are serialized
across processes, and each writes its result to a file the next run compares
against.

Keep the public surface tiny and the machinery private.
