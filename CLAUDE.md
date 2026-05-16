# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Workspace layout

Two-crate Cargo workspace (implicit — `quickbench` depends on `quickbench-macros` via path):

- `quickbench` (`src/lib.rs`) — runtime: `Bencher` builder, `BenchResult`, cross-process named-lock serialization, ANSI-coloured output, optional result file writing + previous-run comparison.
- `quickbench-macros` (`quickbench-macros/src/lib.rs`) — proc-macro crate providing `#[quick_bench]`, which expands to a `#[test]` (by default `#[ignore]`) that constructs a `Bencher` and runs the user's body.

The macro hardcodes `::quickbench::` as the runtime path and passes `env!("CARGO_MANIFEST_DIR")` as the output dir, so results land in `<consumer-crate>/bench-results/<name>.txt`.

## Commands

```bash
cargo build                                  # both crates
cargo test                                   # unit tests (benches are #[ignore] by default)
cargo test -- --ignored --nocapture          # run benchmarks; --nocapture to see [BENCH] output
cargo test --release -- --ignored --nocapture # what users should actually run; debug mode prints a warning
cargo test <name>                            # single test/bench by substring
```

There is no separate `cargo bench` target — benchmarks are `#[test] #[ignore]` functions surfaced via `--ignored`.

## Architecture notes

- **Cross-process serialization** uses `named_lock` with a single global name `quick-bench-lock` (see `BENCH_LOCK_NAME` in `src/lib.rs`). The lock is held for the whole `bench()` call including file I/O, so concurrent `cargo test` invocations on the same machine serialize automatically. Don't introduce a second lock or scope it more narrowly without thinking through the parallel-`cargo test` case.
- **Stop conditions**: warmup and measurement each take an optional `Duration` and an optional iteration cap; the loop exits on whichever fires first. At least one of the two must be set per phase (asserted at runtime). The macro emits `.without_warmup_time()` / `.without_bench_time()` only when the user specified iters but no time, to disable the default 1s/5s limits.
- **Result comparison**: when `output_dir` is set (always, via the macro), `compute_result` reads the previous `median:` line from the per-bench file, prints a coloured diff (>5% threshold for faster/SLOWER), and overwrites the file. `parse_duration` handles `ns`/`us`/`µs`/`ms`/`s` suffixes — keep it in sync with `Debug` formatting of `Duration` if Rust ever changes that.
- **Labeled variants**: `bench_labeled(&self, ...)` clones config and runs a sub-bench named `"{parent}/{label}"`. The `/` becomes a real path separator in the output file, so `compute_result` calls `create_dir_all` on the parent before writing.

## Conventions specific to this repo

- Edition 2024; macro relies on let-chains (`if let ... && cond`) which need that edition.
- Public surface is intentionally tiny: `Bencher`, `BenchResult`, `quick_bench`. The colors module and `parse_duration` are private — keep them that way.
