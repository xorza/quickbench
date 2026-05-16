# quickbench — module review

Scope: the whole workspace (`src/`, `quickbench-macros/`).

## Architectural issues

### 1. Global cross-process lock name is hardcoded and project-specific
`BENCH_LOCK_NAME = "quick-bench-lock"` (`src/lib.rs:24`) is a single global name shared by *every* consumer of this crate on the machine. The doc comment even says "All benchmarks in the Scenarium project will use this lock" — a leak from the previous home of the code. Two unrelated crates that both use quickbench will serialize against each other forever, even if they have nothing to do with one another and the user wanted them to run in parallel.
Suggested change: derive the lock name from `CARGO_PKG_NAME` of the *consumer* (passed in by the macro alongside `CARGO_MANIFEST_DIR`), or expose `Bencher::with_lock_name(&str)` / `without_lock()` and let the macro pass `env!("CARGO_PKG_NAME")` by default. Update the stale doc comment either way.

### 2. `Bencher` is a builder with mandatory post-conditions only checked at run time
`bench()` asserts that at least one of `(warmup_time, warmup_iters)` and one of `(time, iters)` is set (`src/lib.rs:181-188`). The `Default` impl sets `warmup_time` and `time` to `Some`, so calling `.without_warmup_time()` then forgetting `.with_warmup_iters(...)` panics at run time instead of compile time. The macro side-steps this by emitting `.without_warmup_time()` only when `warmup_iters` is given, but the public builder API can still produce a useless `Bencher`.
Suggested change: either drop `without_*` from the public API and require explicit `with_*` on a `Bencher` whose defaults are `None`, or model the two phases as `WarmupSpec` / `BenchSpec` enums (`ByTime`, `ByIters`, `Either`). Macro currently always emits `with_output_dir(env!("CARGO_MANIFEST_DIR"))` and at least one knob, so this only tightens the hand-written path.

### 3. File comparison uses `Debug`-formatted `Duration` as a wire format
`compute_result` writes `"median: {:?}"` (`src/lib.rs:316`) and `read_previous_median` parses it back with a hand-rolled `parse_duration` (`src/lib.rs:356`). `Duration`'s `Debug` is not a stable format — it can render as `1.234s`, `123.4µs`, `1.234ms`, `123ns` depending on magnitude, and the suffix list is closed-source. The parser already needed both `µs` and `us` to cope. A future libstd tweak silently breaks comparisons (no error, just `None`).
Suggested change: write nanoseconds as an integer (`median_ns: 1234567`) alongside (or instead of) the human-readable line. Parse the canonical line; the pretty line is for humans.

### 4. Output is unconditionally written and overwritten — no opt-out, no history
The macro hardcodes `with_output_dir(env!("CARGO_MANIFEST_DIR"))` (`quickbench-macros/src/lib.rs:213`), so every `#[quick_bench]` invocation writes into `<consumer>/bench-results/<name>.txt`, overwriting the previous run after reading exactly one prior median. CI runs trample local results; there's no append, no run id, no machine tag, no way to disable.
Suggested change: (a) let the macro accept `output_dir = "..."` / `no_output` attributes; (b) default the directory to `target/bench-results/` (build artifact, gitignored) rather than the source tree; (c) consider appending a JSONL line per run instead of overwriting, with the "previous" line being the last entry.

### 5. Lock is held across file I/O and `println!`
The named lock is acquired at the start of `bench()` and released at the end of `compute_result` (held through stdout writes, `create_dir_all`, `OpenOptions::open`, `write_all`). Stdout contention with `cargo test`'s test harness or a slow filesystem extends the critical section for everybody waiting on the lock. The measurement itself is the only part that must be serialized.
Suggested change: scope `_guard` to just the warmup+measurement loop; do printing and file I/O after dropping it. Move `compute_result` to take `&self` and call it post-drop.

## Simplifications

### 6. Drop the `colors` module; use a 3-line helper or a tiny crate
Eight `&'static str` constants with manual interpolation produce dense, hard-to-edit format strings (`src/lib.rs:60-69`, `283-293`). Either:
- Use a single `style!(Bold+Cyan, "[BENCH]")` helper, or
- Pull in `anstyle` (zero-dep, libstd-style) and stop hand-rolling ANSI.
Also: no `NO_COLOR` / `isatty` check — colours are emitted even when redirected to a file. Either gate on `std::io::IsTerminal` or accept it and document it.

### 7. `bench_labeled` clones the entire builder for no reason
`bench_labeled` (`src/lib.rs:149-163`) manually copies every field of `self` into a new `Bencher` with a renamed `name`. That's just `Clone` + a name override.
Suggested change: derive `Clone` on `Bencher` (all fields are `Clone`), then `let mut b = self.clone(); b.name = format!("{}/{label}", self.name); b.bench(f)`. Or take `&self` and pass an explicit name into a private `bench_with_name`.

### 8. `compute_result` consumes `self` but only needs `name` + `output_dir`
`fn compute_result(self, ...)` (`src/lib.rs:242`) takes ownership only to move `self.name` into the result. Combined with #5, splitting this into a pure stats function + an I/O function makes both testable in isolation and removes the ownership dance.

### 9. `parse_duration`'s `s` branch eats `ms`/`us`/`ns`
The order in `parse_duration` (`src/lib.rs:356-375`) happens to work because `ms`/`µs`/`us`/`ns` are checked before `s`, but the `s` branch will also accept `"1.2m"` if someone ever adds a minute suffix above it incorrectly, and the suffix family is anyway fragile (see #3). If you keep human-readable parsing, match the suffix once with a small table.

### 10. `Vec::with_capacity(1000)` is a magic guess
`times = Vec::with_capacity(1000)` (`src/lib.rs:221`) — fine but arbitrary. For a 5s bench of a 10ns op you'll do hundreds of millions of pushes and reallocate plenty; for a 100ms-per-iter bench you over-allocate. Either drop the capacity (let Vec grow), or estimate from `iters` when present.

## Smaller improvements

- `lib.rs:23` doc comment "All benchmarks in the Scenarium project" — stale, generalize or delete.
- `BenchResult.total` is computed and stored but never displayed or compared — drop the field or display it.
- `BenchResult.iterations: usize` while iter caps everywhere else are `u64` — pick one.
- `read_previous_median` only reads median; if you ever compare mean/min you'll re-scan the file. Parse the whole record once into a small struct.
- `Display` for `BenchResult` mixes a newline into the middle of the format string; consumers can't get a single-line form. Provide `fn one_line(&self)` or split header/body.
- Macro emits a `#[test]` regardless of `#[cfg(test)]`; works fine because `quick_bench` is only used in test modules in practice, but `#[test]` outside `cfg(test)` is a footgun if someone applies it to a function in a non-test mod. Consider documenting the expectation, or emit `#[cfg(test)] #[test]`.
- Macro error message lists attribute names; `ignore = false` is the only bool, easy to miss in the list of "expected" attributes since they read as numbers. Group them in the docstring's error text.
- No `.gitignore` — `bench-results/` will be committed by default given the manifest-dir output location (see #4).
- `Cargo.toml` is not a workspace manifest; `quickbench-macros` is reached only by the path dependency. If you intend `cargo test` at the root to also test the macros crate, make it a `[workspace]`.
- `named-lock` 0.4 brings in `parking_lot` + libc per platform. For a "test-time only" feature it's a non-trivial dep — acceptable, but worth a comment that it's there only for cross-process serialization (point to #1).
- Macro hardcodes `::quickbench::` (`quickbench-macros/src/lib.rs:206`) — if the user renames the dep in their `Cargo.toml`, the expansion breaks with no good error. Either document the requirement or use `proc-macro-crate` to resolve the real name.
- Bench loop measures `Instant::now()` twice per iteration including the `black_box` overhead; for sub-100ns ops that's most of the measurement. Standard fix is batched iterations (run N inner iterations per timed sample, divide). Not urgent, but the current numbers are misleading for very fast operations.
- `src/tests.rs` re-implements what the macro does, by hand, across six tests. They exist to exercise the builder API, but they're nearly identical — one parameterized test plus one macro-using test would cover the same ground and remove drift between hand-written and macro-emitted patterns.

## Open questions

- Is this crate intended to be published, or used only by one downstream project? The hardcoded lock name (#1), the always-on file output (#4), and the stale doc comment all read as "extracted from one project, not yet generalized."
- Should benchmarks default to running in `--release` only? Right now they print a warning and run regardless. A `#[cfg(debug_assertions)]` `return` (with override) would prevent meaningless numbers landing in `bench-results/`.
- Is the comparison-vs-previous a feature you want long-term, or scaffolding until you wire up something like `cargo-criterion`-style history? It shapes whether #3/#4 are worth doing properly.
- Are labeled variants (`bench_labeled`) used by anyone, or speculative? If unused, delete; if used, the cross-process lock currently re-acquires per label (each `bench` call locks anew) which is correct but means inter-label timing varies with contention.

## Prioritized shortlist (if you said "go")

1. **De-globalize the lock name (#1)** and update the stale doc comment. Single biggest correctness/usability issue once the crate has more than one consumer.
2. **Stabilize the result-file format (#3)** by writing integer nanoseconds; keep the pretty line for humans. Cheap, prevents silent breakage.
3. **Move default output to `target/bench-results/` and make it opt-out (#4).** Stops polluting source trees and CI diffs.
4. **Drop the lock before printing/file I/O (#5)** — small refactor, real reduction in contention.
5. **Replace `bench_labeled`'s manual field copy with `Clone` (#7)** and split `compute_result` into stats + I/O (#8). Pure cleanup, opens the door to testing the math without touching the filesystem.
