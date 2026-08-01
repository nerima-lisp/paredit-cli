//! What `GET /metrics` answers, in Prometheus text exposition format 0.0.4.
//!
//! Hand-written for the same reason the HTTP layer is: the format is three
//! lines per metric — a `# HELP`, a `# TYPE`, and a value — and a client
//! library for that would be a dependency to keep current forever.
//!
//! The cache counters here are the same ones `paredit/cache` reports, so a
//! panic mid-analysis resets them along with the cache it describes. Prometheus
//! treats a counter that goes backwards as a restart and `rate()` handles it,
//! which is why the reset is left alone rather than tracked separately.

use std::fmt::Write as _;
use std::time::Duration;

use super::service::Counters;

#[derive(Debug, Default)]
pub(super) struct Metrics {
    requests: u64,
    /// Summed in nanoseconds and divided out only at render time, so the
    /// exposed total does not drift the way an `f64` accumulator would.
    elapsed_nanos: u128,
}

impl Metrics {
    pub(super) const fn record(&mut self, elapsed: Duration) {
        self.requests = self.requests.saturating_add(1);
        self.elapsed_nanos = self.elapsed_nanos.saturating_add(elapsed.as_nanos());
    }

    pub(super) fn render(&self, cache: Counters) -> String {
        let mut text = String::new();
        metric(
            &mut text,
            "paredit_serve_requests_total",
            "counter",
            "Requests answered since startup, including refused ones.",
            &self.requests.to_string(),
        );
        metric(
            &mut text,
            "paredit_serve_request_duration_seconds_total",
            "counter",
            "Time spent answering those requests.",
            &seconds(self.elapsed_nanos),
        );
        metric(
            &mut text,
            "paredit_serve_cache_hits_total",
            "counter",
            "File analyses answered from the cache.",
            &cache.hits.to_string(),
        );
        metric(
            &mut text,
            "paredit_serve_cache_misses_total",
            "counter",
            "File analyses that had to read and parse the file.",
            &cache.misses.to_string(),
        );
        metric(
            &mut text,
            "paredit_serve_cache_entries",
            "gauge",
            "Files currently held in the cache.",
            &cache.entries.to_string(),
        );
        text
    }
}

fn metric(text: &mut String, name: &str, kind: &str, help: &str, value: &str) {
    let _ = writeln!(
        text,
        "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
    );
}

fn seconds(nanos: u128) -> String {
    format!("{}.{:09}", nanos / 1_000_000_000, nanos % 1_000_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scraper rejects the whole payload if one metric is malformed, so every
    /// name must carry both of its comment lines and exactly one value line.
    #[test]
    fn every_metric_carries_a_help_a_type_and_a_value() {
        let mut metrics = Metrics::default();
        metrics.record(Duration::from_millis(1500));
        metrics.record(Duration::from_millis(500));
        let text = metrics.render(Counters {
            entries: 3,
            hits: 7,
            misses: 2,
        });

        for name in [
            "paredit_serve_requests_total",
            "paredit_serve_request_duration_seconds_total",
            "paredit_serve_cache_hits_total",
            "paredit_serve_cache_misses_total",
            "paredit_serve_cache_entries",
        ] {
            assert!(text.contains(&format!("# HELP {name} ")), "{text}");
            assert!(text.contains(&format!("# TYPE {name} ")), "{text}");
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with(&format!("{name} ")))
                    .count(),
                1,
                "{text}"
            );
        }

        assert!(text.contains("paredit_serve_requests_total 2"), "{text}");
        assert!(text.contains("paredit_serve_cache_hits_total 7"), "{text}");
        assert!(text.contains("paredit_serve_cache_entries 3"), "{text}");
    }

    /// Two milliseconds is 0.002 seconds, not 2 — an accumulator rendered in
    /// the wrong unit is a graph nobody notices is wrong.
    #[test]
    fn durations_are_rendered_as_whole_seconds_and_a_fraction() {
        assert_eq!(seconds(0), "0.000000000");
        assert_eq!(seconds(2_000_000), "0.002000000");
        assert_eq!(seconds(1_500_000_000), "1.500000000");
        assert_eq!(seconds(90_000_000_000), "90.000000000");
    }
}
