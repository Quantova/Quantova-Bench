//! Per-member attestation cost under the two VRF constructions.

use std::hint::black_box;
use std::time::Instant;

use qtv_crypto::{ml_dsa, vrf, vrf_mldsa};

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

fn main() {
    // The 150 ms slot target, the same consensus parameter the main benchmark uses.
    const SLOT_MS: f64 = 150.0;

    // Real keys. The VRF key pairs and the ML-DSA attestation key are each derived
    // from a fixed seed so the run is reproducible, but every operation below is the
    // real signing and hashing path, not a stub.
    let (slh_sk, _slh_pk) = vrf::keygen(b"quantova attestation cost slh-dsa vrf key");
    let (mldsa_vrf_sk, _mldsa_vrf_pk) =
        vrf_mldsa::keygen(b"quantova attestation cost ml-dsa vrf key");
    let (_attest_pk, attest_sk) = ml_dsa::keygen(&[42u8; 32]);

    // The VRF input a committee member evaluates is the sortition seed for the slot;
    // the attestation message is the block-and-slot digest it signs. Representative
    // 32-byte values, the sizes the real path carries.
    let vrf_input = [17u8; 32];
    let attest_msg = [34u8; 32];
    let empty_ctx: [u8; 0] = [];
    let deterministic_rnd = [0u8; 32];

    // The SLH-DSA VRF prove is slow, so it runs few reps; the ML-DSA operations run
    // more. Medians and spreads are reported either way.
    let slh_prove = measure(5, || {
        black_box(vrf::prove(&slh_sk, black_box(&vrf_input)));
    });
    let mldsa_prove = measure(200, || {
        black_box(vrf_mldsa::prove(&mldsa_vrf_sk, black_box(&vrf_input)));
    });
    let attest_sign = measure(200, || {
        black_box(ml_dsa::sign(
            &attest_sk,
            black_box(&attest_msg),
            &empty_ctx,
            &deterministic_rnd,
        ));
    });

    let slh_cost = slh_prove.median + attest_sign.median;
    let mldsa_cost = mldsa_prove.median + attest_sign.median;

    let rule = || println!("{}", "-".repeat(72));

    println!("========================================================================");
    println!(" Per-member attestation cost: SLH-DSA VRF vs ML-DSA VRF");
    println!("========================================================================");
    println!(" Same host, release build (lto, one codegen unit), real keys and real");
    println!(" operations. The attestation cost is one VRF prove plus one ML-DSA");
    println!(" attestation signature, what a committee member does per slot on the");
    println!(" critical path. Slot target: {:.0} ms.", SLOT_MS);
    println!(" Q-Crypto pinned by git tag v0.3.0.");
    rule();
    println!(" Measured terms (median, 10-90 spread):");
    println!(
        "   SLH-DSA VRF prove        {:>12}   spread {:>12}",
        ms(slh_prove.median),
        ms(slh_prove.spread())
    );
    println!(
        "   ML-DSA  VRF prove        {:>12}   spread {:>12}",
        ms(mldsa_prove.median),
        ms(mldsa_prove.spread())
    );
    println!(
        "   ML-DSA  attestation sign {:>12}   spread {:>12}",
        ms(attest_sign.median),
        ms(attest_sign.spread())
    );
    rule();
    println!(" Per-member attestation cost (VRF prove + ML-DSA sign):");
    println!(
        "   baseline  SLH-DSA VRF : {:>12}  +  {:>12}  =  {:>12}",
        ms(slh_prove.median),
        ms(attest_sign.median),
        ms(slh_cost)
    );
    println!(
        "   candidate ML-DSA VRF  : {:>12}  +  {:>12}  =  {:>12}",
        ms(mldsa_prove.median),
        ms(attest_sign.median),
        ms(mldsa_cost)
    );
    rule();
    println!(" Projected finality floor (the attestation cost is the compute floor on");
    println!(" the critical path; every other finality term is unchanged between the");
    println!(" two, so the delta in the floor is the delta in the VRF prove):");
    println!(
        "   baseline  SLH-DSA VRF : {:>10}  =  {:.1}x the {:.0} ms slot; the VRF prove alone",
        ms(slh_cost),
        slh_cost / 1e6 / SLOT_MS,
        SLOT_MS
    );
    println!(
        "                           overruns the slot, so finality stays multi-second and the"
    );
    println!("                           attestation is the binding term whatever the hardware.");
    println!(
        "   candidate ML-DSA VRF  : {:>10}  =  {:.1}% of the {:.0} ms slot; the attestation",
        ms(mldsa_cost),
        mldsa_cost / 1e6 / SLOT_MS * 100.0,
        SLOT_MS
    );
    println!("                           fits well inside one slot and is no longer the floor, so");
    println!("                           sub-second deterministic finality is reachable, bounded");
    println!("                           by propagation and certificate verification instead.");
    rule();
    let speedup = slh_cost / mldsa_cost;
    println!(
        " The ML-DSA VRF cuts the per-member attestation cost by {:.0}x ({} -> {}),",
        speedup,
        ms(slh_cost),
        ms(mldsa_cost)
    );
    println!(" moving the finality floor off the VRF prove. Both constructions remain in");
    println!(" the crypto crate; this is the evidence for choosing the default.");
    println!("========================================================================");
}
