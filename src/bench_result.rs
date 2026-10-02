use std::fmt;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::colors;
use crate::comparison::Comparison;

/// Statistics from a benchmark run.
#[derive(Debug)]
pub struct BenchResult {
    name: String,
    iterations: usize,
    total: Duration,
    mean: Duration,
    min: Duration,
    max: Duration,
    median: Duration,
}

impl BenchResult {
    /// Statistics of the measured iteration times. `times` must hold at least one.
    ///
    /// The mean and an even count's median are exact quotients rounded down to the nanosecond,
    /// the finest step a [`Duration`] — and the clock that measured `times` — has.
    pub(crate) fn from_times(name: String, mut times: Vec<Duration>) -> Self {
        assert!(!times.is_empty(), "`{name}` has no measured iteration");
        times.sort_unstable();
        let count = times.len();
        let total: Duration = times.iter().sum();
        let middle = count / 2;
        let median = if count.is_multiple_of(2) {
            nanos_to_duration(u128::midpoint(
                times[middle - 1].as_nanos(),
                times[middle].as_nanos(),
            ))
        } else {
            times[middle]
        };
        Self {
            name,
            iterations: count,
            total,
            mean: nanos_to_duration(total.as_nanos() / count as u128),
            min: times[0],
            max: times[count - 1],
            median,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn iterations(&self) -> usize {
        self.iterations
    }

    /// The sum of every measured iteration, warmup excluded.
    pub const fn total(&self) -> Duration {
        self.total
    }

    pub const fn mean(&self) -> Duration {
        self.mean
    }

    pub const fn min(&self) -> Duration {
        self.min
    }

    pub const fn max(&self) -> Duration {
        self.max
    }

    /// The middle iteration time, or the mean of the two middle ones for an even count.
    pub const fn median(&self) -> Duration {
        self.median
    }

    #[expect(
        clippy::print_stdout,
        reason = "the report is what a bench run is for, and `--nocapture` shows stdout"
    )]
    pub(crate) fn print(&self) {
        if colors::enabled() {
            use colors::{BOLD, CYAN, DIM, RESET, YELLOW};
            println!(
                "\n{CYAN}{BOLD}[BENCH]{RESET} {BOLD}{}{RESET}:\n{YELLOW}{:?}{RESET} {DIM}(min: {:?}, max: {:?}, median: {:?}, {} iters){RESET}",
                self.name, self.mean, self.min, self.max, self.median, self.iterations
            );
        } else {
            println!("\n{self}");
        }
    }

    /// Write `<output_dir>/bench-results/<name>.txt`, first printing how the median compares with
    /// the one the file held. A failure is reported and the result file skipped: the measurement
    /// itself stands.
    #[expect(
        clippy::print_stderr,
        reason = "a result file that cannot be written must not fail the bench that measured it"
    )]
    pub(crate) fn write(&self, output_dir: &Path) {
        let file_path = output_dir
            .join("bench-results")
            .join(format!("{}.txt", self.name));
        let directory = file_path.parent().expect("a joined file path has a parent");
        if let Err(error) = fs::create_dir_all(directory) {
            eprintln!(
                "Failed to create bench-results directory {}: {error}",
                directory.display()
            );
            return;
        }

        let comparison =
            previous_median(&file_path).and_then(|previous| Comparison::new(previous, self.median));
        if let Some(comparison) = &comparison {
            comparison.print();
        }

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
            self.name,
            self.mean,
            self.min,
            self.max,
            self.median,
            self.iterations,
            self.mean.as_nanos(),
            self.min.as_nanos(),
            self.max.as_nanos(),
            self.median.as_nanos(),
        );
        if let Some(comparison) = comparison {
            content.push_str(&comparison.to_string());
            content.push('\n');
        }

        if let Err(error) = fs::write(&file_path, content) {
            eprintln!(
                "Failed to write benchmark result {}: {error}",
                file_path.display()
            );
        }
    }
}

impl fmt::Display for BenchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[BENCH] {}:\n{:?} (min: {:?}, max: {:?}, median: {:?}, {} iters)",
            self.name, self.mean, self.min, self.max, self.median, self.iterations
        )
    }
}

/// The `median_ns:` a previous run wrote to `file_path`, if the file exists and holds one.
fn previous_median(file_path: &Path) -> Option<Duration> {
    let content = fs::read_to_string(file_path).ok()?;
    content
        .lines()
        .find_map(|line| line.strip_prefix("median_ns:"))
        .and_then(|value| value.trim().parse().ok())
        .map(Duration::from_nanos)
}

/// A whole number of nanoseconds as a [`Duration`], beyond the `u64` range
/// [`Duration::from_nanos`] takes.
fn nanos_to_duration(nanos: u128) -> Duration {
    const NANOS_PER_SEC: u128 = 1_000_000_000;
    Duration::new(
        u64::try_from(nanos / NANOS_PER_SEC).expect("at most the longest measured iteration"),
        (nanos % NANOS_PER_SEC) as u32,
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use crate::bench_result::{BenchResult, previous_median};

    fn ns(nanos: u64) -> Duration {
        Duration::from_nanos(nanos)
    }

    #[test]
    fn statistics_are_exact_at_both_parities() {
        // Even: sorted 10, 20, 30, 40 — total 100, mean 100 / 4 = 25, median (20 + 30) / 2 = 25.
        // Odd: sorted 1, 3, 5 — mean 9 / 3 = 3, median the middle 3.
        // Halves: 1, 2 — mean and median 1.5 ns, rounded down to 1 ns.
        // Seconds: 4 s + 1 ns and 6 s − 1 ns — total 10 s, mean and median 5 s with a carry
        // from the nanoseconds into the seconds.
        for (times, total, mean, min, max, median) in [
            (
                vec![ns(30), ns(10), ns(40), ns(20)],
                ns(100),
                ns(25),
                ns(10),
                ns(40),
                ns(25),
            ),
            (vec![ns(5), ns(1), ns(3)], ns(9), ns(3), ns(1), ns(5), ns(3)),
            (vec![ns(2), ns(1)], ns(3), ns(1), ns(1), ns(2), ns(1)),
            (vec![ns(7)], ns(7), ns(7), ns(7), ns(7), ns(7)),
            (
                vec![Duration::new(5, 999_999_999), Duration::new(4, 1)],
                Duration::from_secs(10),
                Duration::from_secs(5),
                Duration::new(4, 1),
                Duration::new(5, 999_999_999),
                Duration::from_secs(5),
            ),
        ] {
            let count = times.len();
            let result = BenchResult::from_times("stats".into(), times);
            assert_eq!(result.iterations(), count);
            assert_eq!(
                [
                    result.total(),
                    result.mean(),
                    result.min(),
                    result.max(),
                    result.median()
                ],
                [total, mean, min, max, median],
                "{count} times"
            );
        }
    }

    #[test]
    #[should_panic(expected = "`empty` has no measured iteration")]
    fn no_times_is_refused() {
        BenchResult::from_times("empty".into(), Vec::new());
    }

    #[test]
    fn previous_median_reads_the_median_line_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.txt");
        assert_eq!(previous_median(&path), None, "missing file");

        fs::write(&path, "name: x\nmean: 1us\nmedian: 1us\nmedian_ns: 1234\n").unwrap();
        assert_eq!(previous_median(&path), Some(ns(1234)));

        fs::write(&path, "name: x\nmean_ns: 1234\n").unwrap();
        assert_eq!(previous_median(&path), None, "no median line");

        fs::write(&path, "median_ns: 12.5\n").unwrap();
        assert_eq!(previous_median(&path), None, "not a whole number");
    }
}
