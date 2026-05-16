use std::fs;
use std::time::Duration;

use crate::{Bencher, compare_to_previous, read_previous_median_ns};

#[test]
fn iter_cap_stops_at_requested_count() {
    let mut count = 0u64;
    let result = Bencher::new("iter_cap")
        .without_warmup_time()
        .without_bench_time()
        .with_warmup_iters(10)
        .with_iters(50)
        .without_lock()
        .bench(|| count += 1);
    assert_eq!(result.iterations, 50, "should stop at iters cap");
    assert_eq!(count, 10 + 50, "warmup + measured iterations");
}

#[test]
fn time_cap_stops_within_budget() {
    let result = Bencher::new("time_cap")
        .with_warmup_time_ms(5)
        .with_bench_time_ms(20)
        .without_lock()
        .bench(|| std::hint::black_box(1u64 + 1));
    assert!(result.total <= Duration::from_millis(200), "ran far over budget: {:?}", result.total);
    assert!(result.iterations > 0);
}

#[test]
fn read_previous_median_ns_parses_fixture() {
    let dir = std::env::temp_dir().join("quickbench-test-read");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.txt");
    fs::write(
        &path,
        "name: x\nmean: 1us\nmedian: 1us\nmedian_ns: 1234\n",
    )
    .unwrap();
    assert_eq!(read_previous_median_ns(&path), Some(1234));
    let _ = fs::remove_file(&path);
}

#[test]
fn read_previous_median_ns_returns_none_for_missing_file() {
    let path = std::env::temp_dir().join("quickbench-test-missing-xyz");
    let _ = fs::remove_file(&path);
    assert_eq!(read_previous_median_ns(&path), None);
}

#[test]
fn compare_to_previous_classifies_verdict() {
    let prev_ns = 100;
    let same = compare_to_previous(prev_ns, Duration::from_nanos(102));
    assert_eq!(same.verdict, "same");

    let faster = compare_to_previous(prev_ns, Duration::from_nanos(80));
    assert_eq!(faster.verdict, "faster");
    assert!(faster.pct < 0.0);

    let slower = compare_to_previous(prev_ns, Duration::from_nanos(120));
    assert_eq!(slower.verdict, "SLOWER");
    assert!(slower.pct > 0.0);
}

#[test]
fn bench_persists_results_when_output_dir_set() {
    let dir = std::env::temp_dir().join("quickbench-test-persist");
    let _ = fs::remove_dir_all(&dir);
    Bencher::new("persist_check")
        .without_warmup_time()
        .without_bench_time()
        .with_warmup_iters(1)
        .with_iters(3)
        .with_output_dir(&dir)
        .without_lock()
        .bench(|| ());
    let file = dir.join("bench-results").join("persist_check.txt");
    let content = fs::read_to_string(&file).expect("result file should exist");
    assert!(content.contains("name: persist_check"));
    assert!(content.contains("median_ns:"));
    let _ = fs::remove_dir_all(&dir);
}
