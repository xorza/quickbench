use std::fmt;
use std::time::Duration;

use crate::colors;

/// The band, in percent either side of the previous median, reported as [`Verdict::Same`].
///
/// One run's median on a desktop machine moves by a few percent between runs (frequency scaling,
/// background load, cache state), and a single previous median is all there is to compare
/// against, so a smaller change is not told apart from that noise.
const NOISE_PERCENT: f64 = 5.0;

/// How a median compares with the previous run's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Faster,
    Same,
    Slower,
}

impl Verdict {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Faster => "faster",
            Self::Same => "same",
            Self::Slower => "SLOWER",
        }
    }

    const fn color(self) -> &'static str {
        match self {
            Self::Faster => colors::GREEN,
            Self::Same => colors::DIM,
            Self::Slower => colors::RED,
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A median against the one the previous run recorded.
///
/// Its [`Display`](fmt::Display) form is the `vs_previous:` line of a result file.
#[derive(Debug)]
pub(crate) struct Comparison {
    previous: Duration,
    current: Duration,
    percent: f64,
    verdict: Verdict,
}

impl Comparison {
    /// `None` when `previous` is zero: no relative change exists against it.
    pub(crate) fn new(previous: Duration, current: Duration) -> Option<Self> {
        if previous.is_zero() {
            return None;
        }
        let previous_ns = previous.as_nanos() as f64;
        let percent = (current.as_nanos() as f64 - previous_ns) * 100.0 / previous_ns;
        let verdict = if percent < -NOISE_PERCENT {
            Verdict::Faster
        } else if percent > NOISE_PERCENT {
            Verdict::Slower
        } else {
            Verdict::Same
        };
        Some(Self {
            previous,
            current,
            percent,
            verdict,
        })
    }

    #[expect(
        clippy::print_stdout,
        reason = "the report is what a bench run is for, and `--nocapture` shows stdout"
    )]
    pub(crate) fn print(&self) {
        if colors::enabled() {
            println!(
                "  {}vs previous:{} {:?} -> {:?} ({:+.1}%) {}{}{}",
                colors::DIM,
                colors::RESET,
                self.previous,
                self.current,
                self.percent,
                self.verdict.color(),
                self.verdict,
                colors::RESET
            );
        } else {
            println!(
                "  vs previous: {:?} -> {:?} ({:+.1}%) {}",
                self.previous, self.current, self.percent, self.verdict
            );
        }
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "vs_previous: {:?} -> {:?} ({:+.1}%) {}",
            self.previous, self.current, self.percent, self.verdict
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::comparison::{Comparison, Verdict};

    /// Against a previous median of 100 ns, the change in percent is the difference in ns, and
    /// `diff · 100 / 100` is exact in f64 — so ±5 lands on the band's edge, not next to it.
    #[test]
    fn verdict_and_line_follow_the_change_against_the_previous_median() {
        for (current_ns, verdict, line) in [
            (
                80,
                Verdict::Faster,
                "vs_previous: 100ns -> 80ns (-20.0%) faster",
            ),
            (
                94,
                Verdict::Faster,
                "vs_previous: 100ns -> 94ns (-6.0%) faster",
            ),
            (95, Verdict::Same, "vs_previous: 100ns -> 95ns (-5.0%) same"),
            (
                100,
                Verdict::Same,
                "vs_previous: 100ns -> 100ns (+0.0%) same",
            ),
            (
                102,
                Verdict::Same,
                "vs_previous: 100ns -> 102ns (+2.0%) same",
            ),
            (
                105,
                Verdict::Same,
                "vs_previous: 100ns -> 105ns (+5.0%) same",
            ),
            (
                106,
                Verdict::Slower,
                "vs_previous: 100ns -> 106ns (+6.0%) SLOWER",
            ),
            (
                120,
                Verdict::Slower,
                "vs_previous: 100ns -> 120ns (+20.0%) SLOWER",
            ),
        ] {
            let comparison =
                Comparison::new(Duration::from_nanos(100), Duration::from_nanos(current_ns))
                    .unwrap();
            assert_eq!(comparison.verdict, verdict, "{current_ns} ns");
            assert_eq!(comparison.to_string(), line);
        }
    }

    #[test]
    fn a_zero_previous_median_has_no_comparison() {
        assert!(Comparison::new(Duration::ZERO, Duration::from_nanos(5)).is_none());
        assert!(Comparison::new(Duration::ZERO, Duration::ZERO).is_none());
    }
}
