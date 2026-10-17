# Quantova Bench

This repository holds the throughput and finality benchmark for the Quantova stack. It drives a fixed transaction mix through the real chain crates to a real finality certificate, counts only transactions in blocks whose certificate verifies, and prints the measured compute costs together with figures derived from them under stated assumptions. It measures and derives. It does not tune the pipeline to reach a target and it prints no conclusion that the run did not compute. Governed by the crypto policy in the Quantova Specs repository. Commits are authored by the owner only. Dual licensed under Apache 2.0 and MIT.

The benchmark depends on the stack crates by exact git commit. The chain crates and the node come from Quantova-Chain, the machine qtv-vm from QVM, the consensus crates from QRC-CONSENSUS, the one time verifiable random function from QVRF, and the cryptography from Q-Crypto. The pinned commits live in `Cargo.toml`, the resolved commits are recorded in the committed `Cargo.lock`, and `.cargo/config.toml` sets git fetch with the command line git. The run prints the pins it was built with. No classical cryptography is present, and `cargo deny` enforces that.

## What it measures

Measured in every run on the host that runs it are the parallel signature verification of a whole block, the sequential execution of every transaction through qtv-vm, the state root recompute, the production of one attestation, the verification of a real certificate built from a small real committee, the throughput of the post quantum channel, and a sustained loop that verifies blocks back to back for a set duration. Every measured figure is a median over repeated samples with the tenth to ninetieth percentile spread.

Derived from those measurements are the projection of the certificate to the target committee, the modelled finality latency, the batch size and committee size sweeps, the phone class figure and the attribution table. Each derived figure is labelled in the output with the assumptions it uses.

## Assumed inputs

These values are inputs to the model and are not measured by the run. The output labels each of them as assumed or configured.

1. Parallel verification uses four cores. The run also prints the operating system, the architecture and the number of hardware threads the host reports.
2. The uplink is one gigabit per second symmetric and a node holds sixteen gigabytes of memory.
3. The committee is spread evenly across five regions with the one way latency matrix printed at the top of every run, and gossip uses a fanout of eight.
4. Aggregating the certificate costs two milliseconds.
5. The phone class figure multiplies the host measurements by a single thread derate of three.
6. Thirty percent of the transactions are token standard calls and seventy percent are native transfers.

## How to run it

Build and run in release from this directory.

```
cargo run --locked --release --bin qtv-bench
```

Parameters are read from the environment, so a short local run is available.

```
QTV_BENCH_ACCOUNTS=500 QTV_BENCH_BLOCK=500 QTV_BENCH_COMMITTEE_REAL=8 QTV_BENCH_SECS=10 cargo run --locked --release --bin qtv-bench
```

The variables are the funded account count, the block size in transactions, the real committee actually built and measured, and the steady state duration in seconds. The target committee for the projection is the sampler committee budget.

A second binary times one prove of the one time hash based verifiable random function from QVRF and one ML-DSA signature, and prints their sum and its ratio to the slot parameter.

```
cargo run --locked --release --bin attestation_cost
```

## The transaction mix

The workload signs every transaction with a real ML-DSA key derived for a distinct account, and every measured block verifies all of them with the real chain verifier. Native transfers run the node transfer program on qtv-vm. A token call runs a qtv-vm program that performs a freeze check and an allowlist check on both parties, a balance update and an event counter update, because the token contract is not compiled from the language in this benchmark. Senders and recipients are spread over the funded account set so the writes land on distinct state leaves.

## The finality model

Finality is modelled as one attestation round. The leader builds the block, propagates it, validators verify it, validators attest, the attestations disseminate to every node, the certificate is aggregated, and validators verify it. Propagation terms are computed from the assumed latency matrix, the assumed uplink and the measured wire sizes. Compute terms are the measured medians. The model takes the block time to be the finality latency and the chain throughput to be the block size divided by the block time.

A full committee is not built in wall clock time, so the benchmark builds a small real committee, measures the per attestation verification cost and the per attestation wire size on it, and projects both linearly to the target committee. The output states which certificate was measured and which was projected.

## Reading the output

The run ends with an attribution table computed from the figures of that run. It ranks the modelled finality terms by their share, gives the shares of the projected verification slot, and compares the bandwidth ceiling with the measured verification throughput for the real certificate and with the projected throughput for the target committee, naming the lower of each pair as the bound. Figures move between runs and between hosts, so no results are recorded in this file. Rerun the binary for current figures.

## Configuration and reproducibility

The stack crates are pinned by git commit in `Cargo.toml` and `Cargo.lock` is committed. Run `cargo build --locked --release`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` and `cargo deny check` to reproduce the checks the benchmark is held to. The continuous integration workflow runs the same checks with `--locked` and runs `cargo deny` against `deny.toml`.
