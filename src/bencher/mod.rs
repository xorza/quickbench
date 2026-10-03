use std::env;
use std::ffi::OsString;
use std::hint::black_box;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::Once;
use std::time::{Duration, Instant};

use named_lock::NamedLock;

use crate::bench_result::BenchResult;
use crate::colors;

const DEFAULT_LOCK_NAME: &str = "quickbench-default";
const OUTPUT_DIR_ENV: &str = "QUICKBENCH_OUTPUT_DIR";

/// A simple bencher for measuring execution time in tests.
#[derive(Debug, Clone)]
pub struct Bencher {
    name: String,
    warmup: StopRule,
    measure: StopRule,
    output_dir: Option<PathBuf>,
    lock_name: Option<String>,
}

/// When a phase ends: once `time` has elapsed or after `iters` iterations, whichever comes first.
#[derive(Debug, Clone, Copy)]
struct StopRule {
    time: Option<Duration>,
    iters: Option<u64>,
}

impl StopRule {
    const fn is_set(self) -> bool {
        self.time.is_some() || self.iters.is_some()
    }

    const fn reached(self, elapsed: Duration, count: u64) -> bool {
        matches!(self.time, Some(time) if elapsed.as_nanos() >= time.as_nanos())
            || matches!(self.iters, Some(max) if count >= max)
    }
}

impl Default for Bencher {
    fn default() -> Self {
        Self {
            name: String::new(),
            warmup: StopRule {
                time: Some(Duration::from_secs(1)),
                iters: None,
            },
            measure: StopRule {
                time: Some(Duration::from_secs(5)),
                iters: None,
            },
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
    pub const fn with_warmup_time_ms(mut self, ms: u64) -> Self {
        self.warmup.time = Some(Duration::from_millis(ms));
        self
    }

    /// Set the benchmark time in milliseconds.
    #[must_use]
    pub const fn with_bench_time_ms(mut self, ms: u64) -> Self {
        self.measure.time = Some(Duration::from_millis(ms));
        self
    }

    /// Disable warmup time limit (use only iteration count).
    #[must_use]
    pub const fn without_warmup_time(mut self) -> Self {
        self.warmup.time = None;
        self
    }

    /// Disable bench time limit (use only iteration count).
    #[must_use]
    pub const fn without_bench_time(mut self) -> Self {
        self.measure.time = None;
        self
    }

    /// Set the maximum number of warmup iterations.
    #[must_use]
    pub const fn with_warmup_iters(mut self, iters: u64) -> Self {
        self.warmup.iters = Some(iters);
        self
    }

    /// Set the maximum number of benchmark iterations.
    #[must_use]
    pub const fn with_iters(mut self, iters: u64) -> Self {
        self.measure.iters = Some(iters);
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

    /// Disable cross-process serialization: this bench runs without taking the named lock, so
    /// it can overlap another bench's timing.
    #[must_use]
    pub fn without_lock(mut self) -> Self {
        self.lock_name = None;
        self
    }

    /// Run a labeled benchmark variant, named `<name>/<label>`, without consuming self.
    pub fn bench_labeled<F, R>(&self, label: &str, f: F) -> BenchResult
    where
        F: FnMut() -> R,
    {
        self.labeled(label).bench(f)
    }

    /// Run the benchmark.
    ///
    /// Runs warmup iterations until the warmup time is reached or the warmup iteration count is
    /// hit (whichever comes first), then measured iterations until the bench time is reached or
    /// the iteration count is hit.
    ///
    /// The cross-process lock is held only across the warmup + measurement loop; file I/O and
    /// printing happen after it's released.
    ///
    /// # Panics
    ///
    /// If either phase has no stop condition, or the measured phase stops before its first
    /// iteration (an iteration count of zero, a bench time of zero).
    pub fn bench<F, R>(self, f: F) -> BenchResult
    where
        F: FnMut() -> R,
    {
        let origin = Instant::now();
        self.run(f, env::var_os(OUTPUT_DIR_ENV), || origin.elapsed())
    }

    fn labeled(&self, label: &str) -> Self {
        Self {
            name: format!("{}/{label}", self.name),
            ..self.clone()
        }
    }

    /// [`Self::bench`] with the environment's output directory and the clock passed in: `now`
    /// returns the time since a fixed origin.
    fn run<F, R>(
        self,
        mut f: F,
        env_output_dir: Option<OsString>,
        mut now: impl FnMut() -> Duration,
    ) -> BenchResult
    where
        F: FnMut() -> R,
    {
        assert!(
            self.warmup.is_set(),
            "Either warmup_time or warmup_iters must be set"
        );
        assert!(
            self.measure.is_set(),
            "Either bench_time or iters must be set"
        );
        if cfg!(debug_assertions) {
            warn_debug_build();
        }

        let times = {
            let lock = self
                .lock_name
                .as_deref()
                .map(|name| NamedLock::create(name).expect("Failed to create benchmark lock"));
            let _guard = lock
                .as_ref()
                .map(|lock| lock.lock().expect("Failed to acquire benchmark lock"));
            self.measure(&mut f, &mut now)
        };

        let output_dir = output_dir(env_output_dir, self.output_dir);
        let result = BenchResult::from_times(self.name, times);
        result.print();
        if let Some(dir) = output_dir {
            result.write(&dir);
        }
        result
    }

    /// Run the warmup phase, then return the time of each measured iteration.
    fn measure<R>(
        &self,
        f: &mut impl FnMut() -> R,
        now: &mut impl FnMut() -> Duration,
    ) -> Vec<Duration> {
        let start = now();
        let mut count = 0;
        while !self.warmup.reached(since(start, now()), count) {
            black_box(f());
            count += 1;
        }

        let mut times = Vec::new();
        if let StopRule {
            time: None,
            iters: Some(iters),
        } = self.measure
        {
            times.reserve_exact(usize::try_from(iters).expect("the iteration count fits memory"));
        }
        let start = now();
        let mut count = 0;
        loop {
            let before = now();
            if self.measure.reached(since(start, before), count) {
                break;
            }
            black_box(f());
            times.push(since(before, now()));
            count += 1;
        }
        assert!(
            !times.is_empty(),
            "`{}` stopped before its first measured iteration",
            self.name
        );
        times
    }
}

const fn since(start: Duration, now: Duration) -> Duration {
    now.checked_sub(start)
        .expect("the clock never runs backwards")
}

/// The directory results go to: the environment's, when set and not empty, over the builder's.
fn output_dir(env: Option<OsString>, builder: Option<PathBuf>) -> Option<PathBuf> {
    env.filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or(builder)
}

/// Once per process: every `bench` of a debug build would otherwise repeat it.
#[expect(
    clippy::print_stderr,
    reason = "a warning beside the report, which `--nocapture` shows"
)]
fn warn_debug_build() {
    static WARNED: Once = Once::new();
    WARNED.call_once(|| {
        if io::stderr().is_terminal() {
            eprintln!(
                "\n{}{}⚠️  WARNING:{} DEBUG MODE - benchmarks should be run with --release\n",
                colors::YELLOW,
                colors::BOLD,
                colors::RESET
            );
        } else {
            eprintln!("\nWARNING: DEBUG MODE - benchmarks should be run with --release\n");
        }
    });
}

#[cfg(test)]
mod tests;
