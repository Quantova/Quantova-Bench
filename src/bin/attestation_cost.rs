// Copyright 2026 Quantova Inc
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Per-member attestation cost under the one-time committee credential.

use std::hint::black_box;
use std::time::Instant;

use qtv_crypto::{ml_dsa, sha3::shake256};
use qtv_sampler::validator::SamplerValidator;

/// A distribution over nanosecond samples: median and the 10-90 spread.
struct Summary {
    median: f64,
    p10: f64,
    p90: f64,
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
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
    fn of(samples: &[f64]) -> Summary {
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Summary {
            median: percentile(&sorted, 0.5),
            p10: percentile(&sorted, 0.10),
            p90: percentile(&sorted, 0.90),
        }
    }
    fn spread(&self) -> f64 {
        self.p90 - self.p10
    }
}

fn measure(reps: u32, mut f: impl FnMut()) -> Summary {
    // One untimed warm-up so the first sample does not carry cold-cache cost.
    f();
    let mut samples = Vec::with_capacity(reps as usize);
    for _ in 0..reps {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_nanos() as f64);
    }
    Summary::of(&samples)
}

fn ms(ns: f64) -> String {
    format!("{:.3} ms", ns / 1_000_000.0)
}

#[path = "../pins.rs"]
mod pins;

fn main() {
    let slot_ms = qtv_bft::params::SLOT_MS as f64;

    const CREDENTIAL_SLOT: u64 = 7;
    const CREDENTIAL_HOLDER: u64 = 1;
    let mut master_seed = [0u8; 32];
    shake256(
        b"quantova attestation cost one-time credential key",
        &mut master_seed,
    );
    let sampler = SamplerValidator::from_secret(
        CREDENTIAL_HOLDER,
        &master_seed,
        qtv_bft::params::VALIDATOR_STAKE_QTOV,
    );
    let (_attest_pk, attest_sk) = ml_dsa::keygen(&[42u8; 32]);

    let attest_msg = [34u8; 32];
    let empty_ctx: [u8; 0] = [];
    let deterministic_rnd = [0u8; 32];

    let credential = measure(200, || {
        black_box(sampler.reveal(black_box(CREDENTIAL_SLOT)).unwrap());
    });
    let attest_sign = measure(200, || {
        black_box(ml_dsa::sign(
            &attest_sk,
            black_box(&attest_msg),
            &empty_ctx,
            &deterministic_rnd,
        ));
    });

    let attestation_cost = credential.median + attest_sign.median;

    let rule = || println!("{}", "-".repeat(72));

    println!("========================================================================");
    println!(" Per-member attestation cost: one-time committee credential");
    println!("========================================================================");
    println!(
        " Build profile: {}. One credential reveal plus one ML-DSA signature, timed on this host.",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!(" Slot parameter: {:.0} ms.", slot_ms);
    println!(
        " Q-Crypto pinned by git rev {}.",
        pins::short_rev("qtv-crypto")
    );
    rule();
    println!(" Measured terms (median, 10-90 spread):");
    println!(
        "   one-time credential      {:>12}   spread {:>12}",
        ms(credential.median),
        ms(credential.spread())
    );
    println!(
        "   ML-DSA  attestation sign {:>12}   spread {:>12}",
        ms(attest_sign.median),
        ms(attest_sign.spread())
    );
    rule();
    println!(" Per-member attestation cost (credential reveal + ML-DSA sign):");
    println!(
        "   {:>12}  +  {:>12}  =  {:>12}",
        ms(credential.median),
        ms(attest_sign.median),
        ms(attestation_cost)
    );
    rule();
    println!(" Per-member attestation cost as a multiple of the slot parameter:");
    println!(
        "   {:>10}  =  {:.3}x the {:.0} ms slot",
        ms(attestation_cost),
        attestation_cost / 1e6 / slot_ms,
        slot_ms
    );
    println!("========================================================================");
}
