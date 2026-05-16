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

- **Cross-process serialization** uses `named_lock`. The lock name is scoped per-crate via `env!("CARGO_PKG_NAME")` (set by the macro) — unrelated projects don't contend. The lock is held only across warmup + measurement; printing and file I/O happen after release. Don't introduce a second lock or scope it more narrowly without thinking through the parallel-`cargo test` case.
- **Stop conditions**: warmup and measurement each take an optional `Duration` and an optional iteration cap; the loop exits on whichever fires first. At least one of the two must be set per phase (asserted at runtime). The macro emits `.without_warmup_time()` / `.without_bench_time()` only when the user specified iters but no time, to disable the default 1s/5s limits.
- **Result comparison**: when `output_dir` is set (always, via the macro), `write_result` calls `read_previous_median_ns` to parse the `median_ns:` line from the per-bench file, runs `compare_to_previous` to classify the diff (>5% threshold for faster/SLOWER), prints via `print_comparison`, then overwrites the file.
- **Labeled variants**: `bench_labeled(&self, ...)` constructs a sub-`Bencher` named `"{parent}/{label}"` (without `Clone`). The `/` becomes a real path separator in the output file, so `write_result` calls `create_dir_all` on the parent before writing. Each label re-acquires the cross-process lock — labels are NOT atomic with respect to other processes.
- **Output rendering**: `BenchResult: Display` is plain text. ANSI-coloured rendering goes through `print_result` / `print_comparison`, which check `stdout().is_terminal()` and fall back to plain text when redirected.

## Conventions specific to this repo

- Edition 2024; macro relies on let-chains (`if let ... && cond`) which need that edition.
- Public surface is intentionally tiny: `Bencher`, `BenchResult`, `quick_bench`. The `colors` module, `print_result`, `compare_to_previous`, and `read_previous_median_ns` are private — keep them that way.
