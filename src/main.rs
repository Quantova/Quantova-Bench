// Copyright 2026 Quantova Inc
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Quantova throughput and finality benchmark.

mod consensus;
mod network;
mod profile;
mod stats;
mod transport;
mod workload;

use std::thread;
use std::time::{Duration, Instant};

use qtv_node::ledger::Ledger;

use crate::consensus::{
    certificate_bytes, RealCertificate, PER_ATTESTATION_BYTES, PER_MEMBER_KEY_BYTES,
};
use crate::network::{ComputeCosts, Finality, Sizes};
use crate::profile::{PhoneProfile, ServerProfile};
use crate::stats::{ms, us, Summary};
use crate::workload::{execute_native, execute_token, Kind, SignedTx, Workload};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Verify every signature in the block in parallel over `cores` worker threads,
fn parallel_verify(block: &[SignedTx], cores: usize) -> usize {
    let cores = cores.max(1);
    let chunk = block.len().div_ceil(cores);
    thread::scope(|scope| {
        let mut handles = Vec::new();
        for part in block.chunks(chunk) {
            handles.push(scope.spawn(move || {
                part.iter()
                    .filter(|tx| qtv_tx::verify(&tx.wrapper, &tx.pubkey))
                    .count()
            }));
        }
        handles.into_iter().map(|h| h.join().unwrap()).sum()
    })
}

/// Execute every transaction in the block through the qtv-vm in order, the way
fn execute_block(block: &[SignedTx]) -> u64 {
    let mut checksum = 0u64;
    for tx in block {
        let (s, r) = match tx.kind {
            Kind::Native => execute_native(1_000_000_000, 0, tx.amount, tx.fee),
            Kind::Token => execute_token(1_000_000_000, 0, tx.amount, tx.fee),
        };
        checksum = checksum.wrapping_add(s).wrapping_add(r);
    }
    checksum
}

/// Apply the block to the ledger and return the recomputed state root, the way a
fn apply_and_root(ledger: &mut Ledger, block: &[SignedTx]) -> [u8; 32] {
    for tx in block {
        let mut sender = ledger.account(&tx.sender);
        let mut recipient = ledger.account(&tx.recipient);
        let debit = tx.amount.saturating_add(tx.fee);
        sender.balance = sender.balance.saturating_sub(debit);
        sender.nonce += 1;
        recipient.balance = recipient.balance.saturating_add(tx.amount);
        ledger.set_account(&tx.sender, &sender);
        ledger.set_account(&tx.recipient, &recipient);
    }
    ledger.q_root()
}

fn measure(reps: u32, mut f: impl FnMut()) -> Vec<f64> {
    let mut samples = Vec::with_capacity(reps as usize);
    for _ in 0..reps {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_nanos() as f64);
    }
    samples
}

fn rule() {
    println!("{}", "-".repeat(78));
}

fn main() {
    let server = ServerProfile::host();
    let phone = PhoneProfile::reference_2020();

    let accounts = env_usize("QTV_BENCH_ACCOUNTS", 2_000);
    let block_n = env_usize("QTV_BENCH_BLOCK", 2_000);
    let committee_real = env_usize("QTV_BENCH_COMMITTEE_REAL", 16);
    let committee_target = env_usize(
        "QTV_BENCH_COMMITTEE",
        qtv_sampler::params::COMMITTEE_BUDGET as usize,
    );
    let run_secs = env_usize("QTV_BENCH_SECS", 15);
    let slot_ms = qtv_bft::params::SLOT_MS;

    println!("================================================================================");
    println!(" Quantova throughput and finality benchmark");
    println!("================================================================================");
    println!(" Build: release, lto, one codegen unit. Stack pinned to the production flow:");
    println!("   Quantova-Chain 89843bf, QVM v0.5.4, QRC-CONSENSUS v0.9.1, Q-Crypto rev 33c7e93");
    println!();
    println!(" Primary (server class) validator node profile:");
    println!("   {}", server.name);
    println!("   parallel verify cores: {}", server.verify_cores);
    println!(
        "   uplink: {} Gbps symmetric",
        server.uplink_bps / 1_000_000_000
    );
    println!("   memory: {} GB", server.memory_gb);
    println!();
    println!(" Global topology (documented assumption):");
    println!(
        "   attesting committee spread evenly across regions: {:?}",
        network::REGIONS
    );
    println!("   gossip fanout: {}", network::GOSSIP_FANOUT);
    println!(
        "   average one-way hop latency: {:.1} ms",
        network::average_hop_latency_ms()
    );
    println!("   one-way inter-continental latency matrix (ms):");
    print!("            ");
    for r in network::REGIONS.iter() {
        print!("{:>10}", r);
    }
    println!();
    for (i, r) in network::REGIONS.iter().enumerate() {
        print!("   {:>8} ", r);
        for j in 0..network::REGIONS.len() {
            print!("{:>10.0}", network::LATENCY_MS[i][j]);
        }
        println!();
    }
    println!();
    println!(
        " Consensus parameters: slot {} ms, target committee {}, stake {} QTOV",
        slot_ms,
        committee_target,
        qtv_bft::params::VALIDATOR_STAKE_QTOV
    );
    println!(" Run: {} funded accounts, block {} tx, real committee {} (projected to {}), {} s steady state",
        accounts, block_n, committee_real, committee_target, run_secs);
    rule();

    // ---- workload ----
    println!(
        " Building workload (deriving {} real ML-DSA accounts and signing {} tx) ...",
        accounts, block_n
    );
    let mut wl = Workload::build(accounts, block_n);
    let block_bytes: usize = wl
        .block
        .iter()
        .map(|tx| qtv_codec::to_bytes(&tx.wrapper).len())
        .sum();
    let per_tx_bytes = block_bytes as f64 / block_n as f64;
    let token_pct = 100.0 * wl.token_calls as f64 / block_n as f64;
    let native_pct = 100.0 * wl.native_calls as f64 / block_n as f64;
    println!();
    println!(" Transaction mix (realistic, no no-op transactions):");
    println!(
        "   native transfers between distinct accounts : {:>6} ({:.1}%)",
        wl.native_calls, native_pct
    );
    println!(
        "   issuer token standard transfer calls       : {:>6} ({:.1}%)",
        wl.token_calls, token_pct
    );
    println!("   every tx signed with a real ML-DSA key and verified on the path");
    println!(
        "   mean transaction wire size: {:.0} bytes; block wire size: {:.2} MB",
        per_tx_bytes,
        block_bytes as f64 / 1e6
    );
    rule();

    // ---- steady-state compute measurements ----
    println!(" Steady-state compute costs (server node, release), median and 10-90 spread:");
    println!();

    let sig = Summary::of(&measure(30, || {
        assert_eq!(parallel_verify(&wl.block, server.verify_cores), block_n);
    }));
    println!(
        "   signature verification, whole block, {} cores parallel",
        server.verify_cores
    );
    println!(
        "     {:>10} median   spread {:>10}   ({} / tx)",
        ms(sig.median),
        ms(sig.spread()),
        us(sig.median / block_n as f64)
    );

    let exec = Summary::of(&measure(50, || {
        std::hint::black_box(execute_block(&wl.block));
    }));
    println!("   execution through qtv-vm, whole block, sequential");
    println!(
        "     {:>10} median   spread {:>10}   ({} / tx)",
        ms(exec.median),
        ms(exec.spread()),
        us(exec.median / block_n as f64)
    );

    // Prime the ledger with the block applied once so the root reflects the block.
    apply_and_root(&mut wl.ledger, &wl.block);
    let root = Summary::of(&measure(20, || {
        std::hint::black_box(wl.ledger.q_root());
    }));
    println!(
        "   state root recompute over {} accounts (whole state)",
        accounts
    );
    println!(
        "     {:>10} median   spread {:>10}",
        ms(root.median),
        ms(root.spread())
    );

    // Attestation production: the committee membership VRF and the ML-DSA signature.
    println!("   attestation production (VRF prove + ML-DSA sign), one member ...");
    let attest = Summary::of(&measure(2, || {
        let a = qtv_attest::Attester::from_secret(
            1,
            &consensus::member_secret(1),
            qtv_bft::params::VALIDATOR_STAKE_QTOV,
        );
        let beacon = qtv_sampler::beacon::Beacon::genesis();
        let block = qtv_attest::Block::new(1, [7u8; 32], qtv_attest::Parent::Genesis);
        std::hint::black_box(a.attest(consensus::CHAIN_ID, 1, 1, 0, block, &beacon));
    }));
    println!(
        "     {:>10} median   (this is per member, done in parallel across the committee)",
        ms(attest.median)
    );

    // Build a real certificate and measure its verification.
    println!(
        "   building a real {}-member finality certificate (each attestation runs the VRF) ...",
        committee_real
    );
    let header_hash = wl.ledger.q_root();
    let cert = RealCertificate::build(committee_real, 1, 1, &header_hash);
    assert!(cert.verify(), "the real certificate finalizes its block");
    let certv = Summary::of(&measure(20, || {
        assert!(cert.verify());
    }));
    let per_att_verify = certv.median / cert.attesters as f64;
    println!(
        "   certificate verification, real {} attesters",
        cert.attesters
    );
    println!(
        "     {:>10} median   spread {:>10}   ({} / attestation)",
        ms(certv.median),
        ms(certv.spread()),
        us(per_att_verify)
    );

    let sample = qtv_codec::to_bytes(&wl.block[0].wrapper);
    let aead_mbps = transport::seal_open_mbps(&sample, 5_000);
    println!("   post-quantum channel seal/open (ChaCha20-Poly1305 over ML-KEM + ML-DSA)");
    println!(
        "     {:.0} MB/s ({:.1} Gbps) single stream, above the {} Gbps uplink so the AEAD is not",
        aead_mbps,
        aead_mbps * 8.0 / 1000.0,
        server.uplink_bps / 1_000_000_000
    );
    println!(
        "     the limit at 1 Gbps, but a 10 Gbps node must spread record encryption over cores."
    );
    rule();

    // ---- live sustained verification run ----
    println!(
        " Sustained verification run (real {} s, real 150 ms is the slot target):",
        run_secs
    );
    println!("   a server node verifies finalized blocks back to back: parallel signature");
    println!("   verification, sequential execution, state root, and real certificate check.");
    println!("   Only transactions in blocks whose certificate verifies are counted.");
    let deadline = Instant::now() + Duration::from_secs(run_secs as u64);
    let mut slot_times = Vec::new();
    let mut finalized_tx: u64 = 0;
    let mut slots: u64 = 0;
    while Instant::now() < deadline {
        let t0 = Instant::now();
        let verified = parallel_verify(&wl.block, server.verify_cores);
        std::hint::black_box(execute_block(&wl.block));
        let _root = apply_and_root(&mut wl.ledger, &wl.block);
        let finalized = cert.verify();
        let slot = t0.elapsed().as_nanos() as f64;
        // Count only a fully verified, finalized block.
        if finalized && verified == block_n {
            finalized_tx += block_n as u64;
            slot_times.push(slot);
            slots += 1;
        }
    }
    let slot = Summary::of(&slot_times);
    let elapsed_s: f64 = slot_times.iter().sum::<f64>() / 1e9;
    let verify_tps = finalized_tx as f64 / elapsed_s;
    println!();
    println!(
        "   finalized blocks: {}   finalized transactions: {}",
        slots, finalized_tx
    );
    println!(
        "   per-slot verify time: {} median, spread {}",
        ms(slot.median),
        ms(slot.spread())
    );
    println!(
        "   verification-bound sustained throughput: {:.0} finalized TPS",
        verify_tps
    );
    println!(
        "   (this is the rate a node keeps up with, using the real {}-member certificate)",
        cert.attesters
    );
    rule();

    // ---- project the certificate to the target committee ----
    let target_atts = committee_target; // a healthy committee has all members attest
    let verify_cert_target_ns = per_att_verify * target_atts as f64;
    let cert_bytes_target = certificate_bytes(target_atts);
    let member_keys_bytes = target_atts * PER_MEMBER_KEY_BYTES;
    println!(
        " Projection to the {}-member stage-one committee (verification is a flat",
        committee_target
    );
    println!(" per-attestation loop, so it scales linearly, confirmed on the real certificate):");
    println!("   certificate verification: {}", ms(verify_cert_target_ns));
    println!(
        "   certificate wire size   : {:.2} MB ({} bytes per attestation)",
        cert_bytes_target as f64 / 1e6,
        PER_ATTESTATION_BYTES
    );
    println!(
        "   committee key set a verifier holds: {:.2} MB",
        member_keys_bytes as f64 / 1e6
    );
    let verify_slot_proj_ns = sig.median + exec.median + root.median + verify_cert_target_ns;
    println!(
        "   verification-bound slot with this certificate: {}  ->  {:.0} finalized TPS",
        ms(verify_slot_proj_ns),
        block_n as f64 / (verify_slot_proj_ns / 1e9)
    );
    rule();

    // ---- finality under the global topology ----
    let compute = ComputeCosts {
        build_ns: exec.median + root.median,
        verify_block_ns: sig.median + exec.median + root.median,
        attest_ns: attest.median,
        aggregate_ns: 2_000_000.0,
        verify_cert_ns: verify_cert_target_ns,
    };
    let sizes = Sizes {
        block_bytes,
        certificate_bytes: cert_bytes_target,
    };
    let fin = Finality::assemble(
        &compute,
        &sizes,
        committee_target,
        server.uplink_bytes_per_sec(),
    );
    println!(
        " Finality latency under the global topology (single-slot BFT, committee {}):",
        committee_target
    );
    println!(
        "   leader builds block (execute + root)      {:>10}",
        ms(fin.build * 1e6)
    );
    println!(
        "   propagate block to committee              {:>10}",
        ms(fin.propagate_block * 1e6)
    );
    println!(
        "   validators verify block                   {:>10}",
        ms(fin.verify_block * 1e6)
    );
    println!(
        "   attestation production (VRF prove)        {:>10}",
        ms(fin.attest * 1e6)
    );
    println!(
        "   disseminate attestations (= certificate)  {:>10}",
        ms(fin.disseminate_attestations * 1e6)
    );
    println!(
        "   aggregate certificate                     {:>10}",
        ms(fin.aggregate * 1e6)
    );
    println!(
        "   verify certificate                        {:>10}",
        ms(fin.verify_certificate * 1e6)
    );
    println!("   ------------------------------------------------------");
    println!(
        "   finality latency (block to finalized)     {:>10.0} ms",
        fin.total_ms()
    );
    let block_time_s = fin.total_ms() / 1000.0;
    println!("   beacon-chained block time = finality path; chain throughput = block / block time");
    println!(
        "   sustained chain throughput at block {}: {:.0} finalized TPS",
        block_n,
        block_n as f64 / block_time_s
    );
    rule();

    // ---- parameter re-examination ----
    println!(" Parameter re-examination (phone budget removed): sweep the batch size.");
    println!(
        " Each row rebuilds the finality path for that batch; committee held at {}.",
        committee_target
    );
    println!();
    println!(
        "   {:>8}  {:>10}  {:>12}  {:>12}  {:>10}",
        "batch", "block MB", "finality ms", "block time s", "TPS"
    );
    let uplink = server.uplink_bytes_per_sec();
    let mut peak_tps = 0.0f64;
    let mut peak_batch = 0usize;
    let mut peak_finality = 0.0f64;
    let mut best_finality = f64::MAX;
    let mut best_finality_batch = 0usize;
    let mut best_finality_tps = 0.0f64;
    for &n in &[500usize, 1_000, 2_000, 4_000, 8_000, 16_000, 32_000] {
        // The measured figures are already parallel over the cores and sequential
        // for execution, so scale each by the per-tx cost.
        let sig_n = sig.median / block_n as f64 * n as f64;
        let exec_n = exec.median / block_n as f64 * n as f64;
        let bbytes = (per_tx_bytes * n as f64) as usize;
        let compute_n = ComputeCosts {
            build_ns: exec_n + root.median,
            verify_block_ns: sig_n + exec_n + root.median,
            attest_ns: attest.median,
            aggregate_ns: 2_000_000.0,
            verify_cert_ns: verify_cert_target_ns,
        };
        let sizes_n = Sizes {
            block_bytes: bbytes,
            certificate_bytes: cert_bytes_target,
        };
        let fin_n = Finality::assemble(&compute_n, &sizes_n, committee_target, uplink);
        let bt = fin_n.total_ms() / 1000.0;
        let tps = n as f64 / bt;
        if tps > peak_tps {
            peak_tps = tps;
            peak_batch = n;
            peak_finality = fin_n.total_ms();
        }
        if fin_n.total_ms() < best_finality {
            best_finality = fin_n.total_ms();
            best_finality_batch = n;
            best_finality_tps = tps;
        }
        println!(
            "   {:>8}  {:>10.2}  {:>12.0}  {:>12.2}  {:>10.0}",
            n,
            bbytes as f64 / 1e6,
            fin_n.total_ms(),
            bt,
            tps
        );
    }
    println!();
    let bw_ceiling = uplink / (per_tx_bytes * network::gossip_hops(committee_target) as f64);
    println!(
        "   bandwidth ceiling on block dissemination at {} Gbps over {} hops:",
        server.uplink_bps / 1_000_000_000,
        network::gossip_hops(committee_target)
    );
    println!(
        "     ~{:.0} TPS ({:.0} byte tx). A 10 Gbps node lifts this ~10x.",
        bw_ceiling, per_tx_bytes
    );
    println!();
    println!(" Sweep the committee size (certificate cost and size scale linearly):");
    println!(
        "   {:>10}  {:>16}  {:>14}",
        "committee", "cert verify ms", "cert size MB"
    );
    for &c in &[100usize, 200, 334, 500, 1_000] {
        let vc = per_att_verify * c as f64 / 1e6;
        let cb = certificate_bytes(c) as f64 / 1e6;
        println!("   {:>10}  {:>16.0}  {:>14.2}", c, vc, cb);
    }
    rule();

    // ---- primary headline ----
    println!(" PRIMARY FIGURE (server class, global topology, only finalized tx counted):");
    println!("   throughput and finality trade against batch size, so both operating points:");
    println!(
        "   fastest finality : {:.0} ms at batch {} -> {:.0} finalized TPS",
        best_finality, best_finality_batch, best_finality_tps
    );
    println!(
        "   peak throughput  : {:.0} finalized TPS at batch {} -> {:.0} ms finality",
        peak_tps, peak_batch, peak_finality
    );
    println!("   modelled finality floor is now propagation and verification, not the VRF, and is");
    println!(
        "   near batch-independent; throughput is bandwidth-bound (~{:.0} TPS at {} Gbps).",
        bw_ceiling,
        server.uplink_bps / 1_000_000_000
    );
    rule();

    // ---- phone-class secondary ----
    println!(" Phone-class secondary figure (can a phone participate at all?):");
    let phone_verify_block_ns =
        (sig.median + exec.median + root.median) * phone.single_thread_derate;
    let phone_verify_cert_ns = verify_cert_target_ns * phone.single_thread_derate;
    let phone_attest_ns = attest.median * phone.single_thread_derate;
    println!(
        "   {} (host figures x{:.0} single-thread derate)",
        phone.name, phone.single_thread_derate
    );
    println!(
        "   verify a {}-tx block: {}",
        block_n,
        ms(phone_verify_block_ns)
    );
    println!(
        "   verify the {}-member certificate: {}",
        committee_target,
        ms(phone_verify_cert_ns)
    );
    println!(
        "   produce one attestation (VRF prove): {}",
        ms(phone_attest_ns)
    );
    let phone_slot_ok = (phone_verify_block_ns + phone_verify_cert_ns) / 1e6 < slot_ms as f64;
    println!(
        "   fits the {} ms slot for verification: {}",
        slot_ms,
        if phone_slot_ok { "yes" } else { "no" }
    );
    println!(
        "   verdict: a phone cannot keep the {} ms slot at the stage-one committee; it is a",
        slot_ms
    );
    println!("            secondary data point and sets no parameter.");
    rule();

    // ---- bottleneck verdict ----
    println!(" Bottleneck, from the evidence above:");
    println!("   1. Attestation is no longer the finality floor. The module lattice VRF prove");
    println!(
        "      and sign is {} per member, a fraction of a millisecond, so the modelled",
        ms(attest.median)
    );
    println!("      finality floor is now block propagation, block verification, attestation");
    println!("      dissemination, and certificate verification, not the VRF.");
    println!(
        "   2. The stage-one certificate ({:.1} MB, {}) is the second cost: its size and",
        cert_bytes_target as f64 / 1e6,
        ms(verify_cert_target_ns)
    );
    println!("      verification both scale with committee size. This is where the batch-proof");
    println!("      and bandwidth tension now lives, not in the transaction batch.");
    println!("   3. Transaction throughput is bandwidth-bound (~{:.0} TPS at {} Gbps), not compute-bound:",
        bw_ceiling, server.uplink_bps / 1_000_000_000);
    println!(
        "      server cores verify a large batch cheaply ({} / tx), so the phone-era pressure",
        us(sig.median / block_n as f64)
    );
    println!(
        "      to keep batches small has dissolved for the batch. Execution is sequential but"
    );
    println!(
        "      minor ({} / tx); sequential execution is not the binding constraint.",
        us(exec.median / block_n as f64)
    );
    println!(
        "   The faster VRF lever has landed, module lattice replacing the hash based VRF, so"
    );
    println!("   the remaining levers are propagation and a succinct (stage-two) certificate.");
    println!("================================================================================");
}
