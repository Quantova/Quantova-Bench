// Copyright 2026 Quantova Inc
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Summary statistics over a set of timing samples. The benchmark reports the

/// A distribution over samples in nanoseconds.
pub struct Summary {
    pub median: f64,
    pub p10: f64,
    pub p90: f64,
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

impl Summary {
    /// Summarize a set of nanosecond samples.
    pub fn of(samples: &[f64]) -> Summary {
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Summary {
            median: percentile(&sorted, 0.5),
            p10: percentile(&sorted, 0.10),
            p90: percentile(&sorted, 0.90),
        }
    }

    /// The interquartile-style spread reported here, the tenth to ninetieth
    pub fn spread(&self) -> f64 {
        self.p90 - self.p10
    }
}

/// Format a nanosecond figure in milliseconds.
pub fn ms(ns: f64) -> String {
    format!("{:.3} ms", ns / 1_000_000.0)
}

/// Format a nanosecond figure in microseconds.
pub fn us(ns: f64) -> String {
    format!("{:.2} us", ns / 1_000.0)
}
