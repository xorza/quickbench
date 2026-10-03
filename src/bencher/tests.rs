use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::bencher::{Bencher, output_dir};

/// A clock that reads 0 ms, 1 ms, 2 ms, … on successive calls.
fn ticking_clock() -> impl FnMut() -> Duration {
    let mut tick = 0;
    move || {
        let now = Duration::from_millis(tick);
        tick += 1;
        now
    }
}

fn real_clock() -> impl FnMut() -> Duration {
    let origin = Instant::now();
    move || origin.elapsed()
}

/// Under [`ticking_clock`] the warmup reads its start at 0 ms and checks at 1, 2, 3, … ms, so a
/// `w` ms warmup runs `w − 1` iterations. The measured phase reads its start, then twice per
/// iteration (before and after), so iteration `k` is checked at `1 + 2k` ms elapsed and lasts
/// 1 ms: a `t` ms phase runs the `k` with `1 + 2k < t`, which is `⌈(t − 1) / 2⌉` of them.
#[test]
fn each_phase_stops_at_its_first_reached_condition() {
    let base = || {
        Bencher::new("stop")
            .without_lock()
            .without_warmup_time()
            .without_bench_time()
    };
    for (bencher, warmup, measured, reason) in [
        (
            base().with_warmup_time_ms(5).with_bench_time_ms(20),
            4,
            10,
            "time only: 5 − 1, ⌈19 / 2⌉",
        ),
        (
            base().with_warmup_time_ms(8).with_bench_time_ms(9),
            7,
            4,
            "other times: 8 − 1, ⌈8 / 2⌉",
        ),
        (
            base()
                .with_warmup_time_ms(5)
                .with_warmup_iters(2)
                .with_bench_time_ms(20)
                .with_iters(3),
            2,
            3,
            "iterations first",
        ),
        (
            base()
                .with_warmup_time_ms(3)
                .with_warmup_iters(100)
                .with_bench_time_ms(4)
                .with_iters(100),
            2,
            2,
            "time first: 3 − 1, ⌈3 / 2⌉",
        ),
        (base().with_warmup_iters(0).with_iters(1), 0, 1, "no warmup"),
    ] {
        let mut calls = 0;
        let result = bencher.run(|| calls += 1, None, ticking_clock());
        assert_eq!(
            (calls - measured, result.iterations()),
            (warmup, measured),
            "{reason}"
        );
        let measured = u32::try_from(measured).unwrap();
        assert_eq!(
            result.total(),
            Duration::from_millis(1) * measured,
            "{reason}"
        );
        assert_eq!(result.median(), Duration::from_millis(1), "{reason}");
    }
}

#[test]
fn iteration_counts_hold_on_the_real_clock() {
    let mut calls = 0u64;
    let result = Bencher::new("iter_cap")
        .without_warmup_time()
        .without_bench_time()
        .with_warmup_iters(10)
        .with_iters(50)
        .without_lock()
        .run(|| calls += 1, None, real_clock());
    assert_eq!(result.iterations(), 50);
    assert_eq!(calls, 10 + 50);
}

#[test]
#[should_panic(expected = "`empty` stopped before its first measured iteration")]
fn a_measured_phase_without_iterations_is_refused() {
    Bencher::new("empty")
        .without_lock()
        .with_warmup_iters(0)
        .with_iters(0)
        .run(|| (), None, ticking_clock());
}

#[test]
fn the_environment_overrides_the_builder_unless_empty() {
    let env = || Some(OsString::from("/env"));
    let builder = || Some(PathBuf::from("/builder"));
    for (env, builder, expected) in [
        (None, None, None),
        (None, builder(), builder()),
        (env(), None, Some(PathBuf::from("/env"))),
        (env(), builder(), Some(PathBuf::from("/env"))),
        (Some(OsString::new()), builder(), builder()),
        (Some(OsString::new()), None, None),
    ] {
        assert_eq!(
            output_dir(env.clone(), builder.clone()),
            expected,
            "{env:?} {builder:?}"
        );
    }
}

/// The second run reads the first run's 1 ms median back and records an unchanged one.
#[test]
fn results_persist_and_the_next_run_compares_against_them() {
    let dir = tempfile::tempdir().unwrap();
    let bencher = Bencher::new("persist")
        .with_warmup_iters(1)
        .without_warmup_time()
        .with_iters(3)
        .without_bench_time()
        .with_output_dir(dir.path())
        .without_lock();
    let file = dir
        .path()
        .join("bench-results")
        .join("persist")
        .join("variant.txt");

    bencher.labeled("variant").run(|| (), None, ticking_clock());
    let first = fs::read_to_string(&file).unwrap();
    assert_eq!(
        first,
        "name: persist/variant\nmean: 1ms\nmin: 1ms\nmax: 1ms\nmedian: 1ms\niterations: 3\n\
         mean_ns: 1000000\nmin_ns: 1000000\nmax_ns: 1000000\nmedian_ns: 1000000\n"
    );

    bencher.labeled("variant").run(|| (), None, ticking_clock());
    let second = fs::read_to_string(&file).unwrap();
    assert_eq!(
        second,
        format!("{first}vs_previous: 1ms -> 1ms (+0.0%) same\n")
    );
}
