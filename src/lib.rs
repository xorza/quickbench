//! Simple benchmarking utilities for use in tests.
//!
//! This crate provides a lightweight benchmarking framework that runs as regular tests
//! but measures execution time with proper warmup and statistics.
//!
//! ## Cross-Process Serialization
//!
//! Benchmarks are serialized across processes using a named system lock. By default the
//! lock name is scoped per-crate (the `#[quick_bench]` macro passes `CARGO_PKG_NAME`), so
//! unrelated projects don't contend with each other. Override with [`Bencher::with_lock_name`]
//! or disable with [`Bencher::without_lock`].

use std::fs::{OpenOptions, create_dir_all, read_to_string};
use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use named_lock::NamedLock;

pub use quickbench_macros::quick_bench;

const DEFAULT_LOCK_NAME: &str = "quickbench-default";

mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const CYAN: &str = "\x1b[36m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const GREEN: &str = "\x1b[32m";
    pub const RED: &str = "\x1b[31m";
    pub const DIM: &str = "\x1b[2m";
}

/// A simple bencher for measuring execution time in tests.
#[derive(Debug, Clone)]
pub struct Bencher {
    name: String,
    warmup_time: Option<Duration>,
    time: Option<Duration>,
    warmup_iters: Option<u64>,
    iters: Option<u64>,
    output_dir: Option<PathBuf>,
    lock_name: Option<String>,
}

/// Statistics from a benchmark run.
#[derive(Debug)]
pub struct BenchResult {
    pub name: String,
    pub iterations: usize,
    pub total: Duration,
    pub mean: Duration,
    pub min: Duration,
    pub max: Duration,
    pub median: Duration,
}

impl std::fmt::Display for BenchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use colors::*;
        write!(
            f,
            "{CYAN}{BOLD}[BENCH]{RESET} {BOLD}{}{RESET}:\n{YELLOW}{:?}{RESET} {DIM}(min: {:?}, max: {:?}, median: {:?}, {} iters){RESET}",
            self.name, self.mean, self.min, self.max, self.median, self.iterations
        )
    }
}

impl Default for Bencher {
    fn default() -> Self {
        Self {
            name: String::new(),
            warmup_time: Some(Duration::from_secs(1)),
            time: Some(Duration::from_secs(5)),
            warmup_iters: None,
            iters: None,
            output_dir: None,
            lock_name: Some(DEFAULT_LOCK_NAME.to_string()),
        }
    }
}

impl Bencher {
    /// Create a new bencher with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the warmup time in milliseconds.
    #[must_use]
    pub fn with_warmup_time_ms(mut self, ms: u64) -> Self {
        self.warmup_time = Some(Duration::from_millis(ms));
        self
    }

    /// Set the benchmark time in milliseconds.
    #[must_use]
    pub fn with_bench_time_ms(mut self, ms: u64) -> Self {
        self.time = Some(Duration::from_millis(ms));
        self
    }

    /// Disable warmup time limit (use only iteration count).
    #[must_use]
    pub fn without_warmup_time(mut self) -> Self {
        self.warmup_time = None;
        self
    }

    /// Disable bench time limit (use only iteration count).
    #[must_use]
    pub fn without_bench_time(mut self) -> Self {
        self.time = None;
        self
    }

    /// Set the maximum number of warmup iterations.
    #[must_use]
    pub fn with_warmup_iters(mut self, iters: u64) -> Self {
        self.warmup_iters = Some(iters);
        self
    }

    /// Set the maximum number of benchmark iterations.
    #[must_use]
    pub fn with_iters(mut self, iters: u64) -> Self {
        self.iters = Some(iters);
        self
    }

    /// Set the output directory (a `bench-results/` subdirectory will be created inside).
    #[must_use]
    pub fn with_output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    /// Override the cross-process lock name. Benchmarks sharing a name serialize together.
    #[must_use]
    pub fn with_lock_name(mut self, name: impl Into<String>) -> Self {
        self.lock_name = Some(name.into());
        self
    }

    /// Disable cross-process serialization entirely.
    #[must_use]
    pub fn without_lock(mut self) -> Self {
        self.lock_name = None;
        self
    }

    /// Run a labeled benchmark variant without consuming self.
    pub fn bench_labeled<F, R>(&self, label: &str, f: F) -> BenchResult
    where
        F: FnMut() -> R,
    {
        let mut b = self.clone();
        b.name = format!("{}/{label}", self.name);
        b.bench(f)
    }

    /// Run the benchmark.
    ///
    /// Runs warmup iterations until warmup_time is reached or warmup_iters is hit (whichever
    /// comes first), then runs benchmark iterations until bench_time is reached or iters is hit.
    /// At least one stop condition must be set per phase.
    ///
    /// The cross-process lock is held only across the warmup + measurement loop; file I/O and
    /// printing happen after it's released.
    pub fn bench<F, R>(self, mut f: F) -> BenchResult
    where
        F: FnMut() -> R,
    {
        assert!(
            self.warmup_time.is_some() || self.warmup_iters.is_some(),
            "Either warmup_time or warmup_iters must be set"
        );
        assert!(
            self.time.is_some() || self.iters.is_some(),
            "Either bench_time or iters must be set"
        );

        #[cfg(debug_assertions)]
        println!(
            "\n{}{}⚠️  WARNING:{} DEBUG MODE - benchmarks should be run with --release\n",
            colors::YELLOW,
            colors::BOLD,
            colors::RESET
        );

        let times = {
            let lock = self
                .lock_name
                .as_deref()
                .map(|name| NamedLock::create(name).expect("Failed to create benchmark lock"));
            let _guard = lock
                .as_ref()
                .map(|l| l.lock().expect("Failed to acquire benchmark lock"));

            // Warmup
            let warmup_start = Instant::now();
            let mut warmup_count = 0u64;
            loop {
                if let Some(time) = self.warmup_time
                    && warmup_start.elapsed() >= time
                {
                    break;
                }
                if let Some(max) = self.warmup_iters
                    && warmup_count >= max
                {
                    break;
                }
                black_box(f());
                warmup_count += 1;
            }

            // Timed runs
            let mut times = Vec::new();
            let bench_start = Instant::now();
            loop {
                if let Some(time) = self.time
                    && bench_start.elapsed() >= time
                {
                    break;
                }
                if let Some(max) = self.iters
                    && times.len() as u64 >= max
                {
                    break;
                }
                let start = Instant::now();
                black_box(f());
                times.push(start.elapsed());
            }

            drop(_guard);
            drop(lock);
            times
        };

        let result = compute_stats(self.name, times);
        println!("\n{result}");
        if let Some(dir) = self.output_dir.as_deref() {
            write_result(&result, dir);
        }
        result
    }
}

fn compute_stats(name: String, mut times: Vec<Duration>) -> BenchResult {
    times.sort();
    let total: Duration = times.iter().sum();
    let n = times.len().max(1) as u32;
    let mean = total / n;
    let min = times.first().copied().unwrap_or_default();
    let max = times.last().copied().unwrap_or_default();
    let median = times.get(times.len() / 2).copied().unwrap_or_default();
    BenchResult {
        name,
        iterations: times.len(),
        total,
        mean,
        min,
        max,
        median,
    }
}

fn write_result(result: &BenchResult, output_dir: &Path) {
    let bench_dir = output_dir.join("bench-results");
    if let Err(e) = create_dir_all(&bench_dir) {
        eprintln!("Failed to create bench-results directory: {e}");
        return;
    }
    let file_path = bench_dir.join(format!("{}.txt", result.name));
    if let Some(parent) = file_path.parent() {
        let _ = create_dir_all(parent);
    }

    let comparison = read_previous_median_ns(&file_path).map(|prev_ns| {
        let cur_ns = result.median.as_nanos() as f64;
        let prev = prev_ns as f64;
        let diff = cur_ns - prev;
        let pct = (diff / prev) * 100.0;
        let sign = if diff >= 0.0 { "+" } else { "" };
        let (indicator, color) = if pct < -5.0 {
            ("faster", colors::GREEN)
        } else if pct > 5.0 {
            ("SLOWER", colors::RED)
        } else {
            ("same", colors::DIM)
        };
        let prev_dur = Duration::from_nanos(prev_ns);
        println!(
            "  {}vs previous:{} {:?} -> {:?} ({sign}{:.1}%) {}{}{}",
            colors::DIM,
            colors::RESET,
            prev_dur,
            result.median,
            pct,
            color,
            indicator,
            colors::RESET
        );
        format!(
            "vs_previous: {:?} -> {:?} ({sign}{:.1}%) {indicator}",
            prev_dur, result.median, pct
        )
    });

    let mut content = format!(
        "name: {}\n\
         mean: {:?}\n\
         min: {:?}\n\
         max: {:?}\n\
         median: {:?}\n\
         iterations: {}\n\
         mean_ns: {}\n\
         min_ns: {}\n\
         max_ns: {}\n\
         median_ns: {}\n",
        result.name,
        result.mean,
        result.min,
        result.max,
        result.median,
        result.iterations,
        result.mean.as_nanos(),
        result.min.as_nanos(),
        result.max.as_nanos(),
        result.median.as_nanos(),
    );
    if let Some(cmp) = comparison {
        content.push_str(&cmp);
        content.push('\n');
    }

    match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&file_path)
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(content.as_bytes()) {
                eprintln!("Failed to write benchmark result: {e}");
            }
        }
        Err(e) => eprintln!("Failed to open benchmark results file: {e}"),
    }
}

fn read_previous_median_ns(file_path: &Path) -> Option<u64> {
    let content = read_to_string(file_path).ok()?;
    content
        .lines()
        .find_map(|line| line.strip_prefix("median_ns:"))
        .and_then(|v| v.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests;
