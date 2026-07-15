# Quantova Bench

This repository holds the throughput and finality benchmark for the Quantova stack. It drives a realistic transaction mix through the real chain crates to a real finality certificate, holds the verifying side to a documented server class validator node, counts only transactions that actually finalize, and reports the sustained transactions per second, the finality latency under a global topology, a phone class secondary figure, and an evidence based attribution of the bottleneck. It measures and attributes. It does not tune the pipeline to reach a target. Governed by the crypto policy in the Quantova Specs repository. Commits are authored by the owner only. Dual licensed under Apache 2.0 and MIT.

The benchmark depends on the stack crates by git tag. The chain crates and the node come from Quantova-Chain v0.4.0, the machine qtv-vm from QVM v0.2.0, the consensus crates from QRC-CONSENSUS v0.2.0, and the cryptography from Q-Crypto v0.3.0. The pins live in `Cargo.toml` and `.cargo/config.toml` sets git fetch with the command line git. No classical cryptography is present, and `cargo deny` enforces that.

## What it measures

The benchmark reports the sustained transactions per second the network holds, the finality latency from a block being built to it being finalized, and where the time goes at steady state across signature verification, execution, state root, attestation, certificate verification, and networking. Every figure is a median over many samples with the tenth to ninetieth percentile spread, not a single run.

## How to run it

Build and run in release from this directory.

```
cargo run --release
```

The run takes a few minutes because it builds a real committee, and each attestation runs the committee membership verifiable random function once, which is slow at this parameter set. Parameters are read from the environment, so a short local run is available.

```
QTV_BENCH_ACCOUNTS=500 QTV_BENCH_BLOCK=500 QTV_BENCH_COMMITTEE_REAL=8 QTV_BENCH_SECS=10 cargo run --release
```

The variables are the funded account count, the block size in transactions, the real committee actually built and measured, and the steady state duration in seconds. The target committee for the projection is the sampler committee budget, which is 500.

A second binary reports the per member attestation cost under each of the two verifiable random function constructions, the hash based one on SLH-DSA and the lattice based one on ML-DSA, so the founder can compare them on the same host and choose the default. It measures one verifiable random function prove and one ML-DSA attestation signature, which is the work a committee member does per slot on the critical path.

```
cargo run --release --bin attestation_cost
```

## The transaction mix

The workload is 70 percent native transfers between distinct accounts and 30 percent calls against the issuer token standard. Both sides move value and mutate two accounts, so no transaction is a no op. Each sender and recipient is drawn from a large account set so the signatures are over distinct keys and the writes land on distinct state leaves, which means no hot account caching flatters the result.

The ratio is chosen to represent a settlement chain whose anchor tenant is a regulated stablecoin issued under the token standard. The bulk of activity on such a chain is plain value transfer between many distinct accounts, and a large minority is token transfer that carries the issuer compliance checks. Native transfers dominate and token activity is a real and heavier minority, which the 70 to 30 split captures.

Every transaction is signed with a real ML-DSA key and verified with the real chain verifier on the path. Verification is never skipped. The token contract state is stubbed in the stack until the language compiler milestone, so a token transfer is modelled as a real qtv-vm program that runs the issuer standard transfer, a freeze check and an allowlist check on both parties, a conserved balance flow, and a transfer event. It carries the same signature and verification cost as a native transfer and a heavier execution cost.

## The server class node profile

Validators are permissionless and server class, globally distributed. The primary figure holds the verifying side to a server node profile. The reference host is an Apple M4 with ten cores, four performance and six efficiency, and sixteen gigabytes of memory, treated as a modest server node. The benchmark bounds parallel verification to four cores, which is below a real server core count on purpose, so the parallel figures are conservative. The modelled uplink is one gigabit per second symmetric. A real validator node is typically larger than this, so the honest reading of the compute figures is that they are an upper bound on the time and a lower bound on the throughput a real node reaches. The benchmark never runs the verifying side on hardware more generous than a modest server node.

## The global topology

The topology is documented so the finality figure survives scrutiny. The attesting committee is spread evenly across five regions, North America, South America, Europe, Asia, and Oceania. Latency between regions is a symmetric one way matrix of representative inter continental figures, from typical public cloud region measurements with the round trip halved. Intra region latency is five milliseconds and inter region latency ranges from forty to one hundred sixty milliseconds. The average one way hop latency across the topology is about sixty eight milliseconds. Each node has the modelled one gigabit per second uplink. Gossip uses a fanout of eight, so a payload reaches the committee in a logarithmic number of hops, and each hop pays one representative link latency and the time to move the payload at the uplink rate. The exact matrix is printed at the top of every run.

Finality is modelled as one attestation round of the stage one byzantine fault tolerant core. The leader builds the block, propagates it, validators verify it, validators attest, the attestations disseminate to every node which is the certificate itself, the certificate aggregates, and validators verify it. Because committee selection for the next height draws on the certificate of the current height, the heights are serial, so the block time equals the finality path and the chain throughput is the block size divided by the block time.

## The measurement method

The compute costs are measured directly on the real crates in release. Signature verification runs the real chain verifier in parallel over the bounded cores. Execution runs the real qtv-vm interpreter for each transaction. The state root runs the real sparse trie over the whole account set. Attestation runs the real committee member, which is one verifiable random function evaluation and one ML-DSA signature. The certificate is a real aggregate of real committee attestations, and its verification is the exact check a validator runs to accept a block as final.

A full committee cannot be built in wall clock time because each attestation runs the verifiable random function once, which is slow at this parameter set, so the benchmark builds a small real committee, measures the per attestation verification cost and the per attestation wire size on it, and projects both linearly to the 500 member committee. The projection is exact in shape because certificate verification is a flat loop over the attestations and the certificate is their concatenation.

A steady state verification run drives a server node verifying finalized blocks back to back over a real duration. It counts only the transactions in a block whose certificate verifies. This is the rate at which a node keeps up with the chain, and it cross checks the assembled compute figures against a real wall clock loop.

## Honesty rules

The number measured is the number reported. The mix is realistic and carries no no op transactions. The steady state is measured over a real duration and reported as a median with its spread, never a peak burst. Only transactions in blocks whose finality certificate verifies are counted. The verifying side is never run on hardware more generous than a modest server node. Where a phone class figure is derated from the host, the raw host figures are reported alongside and the derate is stated, so it can be re applied by an independent checker. A lower honest figure is preferred over a flattering one from a soft method.

## Results on the reference host

The figures below are one authoritative run on the reference host. They move a little between runs, mostly because the verifiable random function proving time and the state root recompute vary, so they are read as representative rather than exact. Rerun the binary for current figures.

The mix was 1400 native transfers and 600 token calls in a 2000 transaction block. A transaction is about 3522 bytes on the wire, dominated by the ML-DSA signature, and the block is about 7 megabytes.

Signature verification is about 45 microseconds per transaction over four cores. Execution is about 10 microseconds per transaction and is sequential. The state root over 2000 accounts is about 190 milliseconds and scales linearly with the account set because the current trie recomputes the whole root each block. Attestation production is 1.4 to 1.9 seconds, which is one SLH-DSA-192s signature for the committee membership function. Certificate verification is about 1.3 milliseconds per attestation, so a 500 member certificate verifies in about 630 milliseconds, and that certificate is about 9.8 megabytes on the wire. The post quantum channel seals and opens at about 200 megabytes per second on a single stream, above the one gigabit uplink, so the wire encryption is not the limit at one gigabit, though a ten gigabit node must spread record encryption over its cores.

The steady state verification run finalized about 116000 transactions across 58 blocks in twenty seconds, at a per slot verification time near 340 milliseconds, which is a verification bound throughput near 5800 finalized transactions per second using the real small committee certificate. With the 500 member certificate the verification bound slot is near 930 milliseconds, near 2100 finalized transactions per second.

Under the global topology, at a 2000 transaction block and a 500 member committee, the finality latency is about 3.9 seconds. Its terms are the leader build near 210 milliseconds, block propagation near 370 milliseconds, block verification near 300 milliseconds, attestation production near 1.9 seconds, attestation dissemination near 440 milliseconds, aggregation near 2 milliseconds, and certificate verification near 630 milliseconds. Sweeping the batch size, throughput rises with the batch toward a bandwidth ceiling near 11800 finalized transactions per second at one gigabit and three gossip hops, while finality rises from about 3.7 seconds at a 500 transaction batch to about 8.4 seconds at a 32000 transaction batch. A ten gigabit node lifts the throughput ceiling about tenfold.

The primary figures are two operating points. The fastest finality is about 3.7 seconds at a small batch, near 140 finalized transactions per second. The peak throughput in the swept range is near 3800 finalized transactions per second at a large batch, at about 8.4 seconds finality. The finality floor is set by stage one consensus and is near independent of the batch. The throughput is bandwidth bound and near independent of compute.

## Phone class secondary figure

The phone class figure is kept only to answer whether a phone could participate at all. It sets no parameter. Deriving the host figures by a conservative threefold single thread slowdown for a 2020 reference phone, verifying a 2000 transaction block takes about 900 milliseconds, verifying the 500 member certificate takes about 1.9 seconds, and producing one attestation takes about 5.8 seconds. None of these fits the 150 millisecond slot, so a phone cannot keep the slot at the stage one committee. This is expected once the validator is server class and is reported as a data point only.

## The bottleneck and the levers

The finality floor is the attestation verifiable random function proving time, one SLH-DSA-192s signature per member per slot on the critical path, which is 1.4 to 1.9 seconds. Finality cannot go sub second at stage one whatever the hardware or the network. The second cost is the stage one certificate, about 9.8 megabytes and about 630 milliseconds to verify at a 500 member committee, both scaling with the committee size. This is where the batch and bandwidth tension now sits, in the certificate, not in the transaction batch. Transaction throughput is bandwidth bound rather than compute bound, because server cores verify a large batch cheaply, so the earlier pressure to keep batches small has dissolved for the batch. Execution is sequential but small at about 10 microseconds per transaction, so sequential execution is not the binding constraint, which the data confirms rather than assumes.

The levers that follow from this are a faster committee membership function and a succinct stage two certificate. A succinct certificate is a constant small proof in place of the aggregated attestations, which collapses the certificate size, its propagation, and its verification at once. These are design decisions on the roadmap and are not implemented here, because this pass measures and attributes the baseline and does not optimize it.

## Attestation cost under the two verifiable random function constructions

The specification defines two verifiable random function constructions on one interface and leaves the choice of default to this benchmark. The baseline is the hash based function on SLH-DSA. The candidate is the lattice based function on ML-DSA, now implemented in the crypto crate alongside the baseline. Neither construction is removed. The per member attestation cost is the operation a committee member runs per slot on the critical path, one verifiable random function prove and one ML-DSA attestation signature, and that cost is the compute floor on finality because the attestation runs once on the critical path in parallel across the committee, so one member cost counts rather than the sum over members.

The `attestation_cost` binary measures both terms on this host with real keys and real operations, the same honest method as the rest of the benchmark, with no tuning to a target. On the reference host a representative run reads as follows.

The SLH-DSA verifiable random function prove is about 1.36 seconds in this run and floats up toward 1.9 seconds across runs, the same SLH-DSA-192s prove the main figures report. The ML-DSA verifiable random function prove is about 0.26 milliseconds. The ML-DSA attestation signature is about 0.19 milliseconds and is the same operation under both constructions, so only the prove term changes. The per member attestation cost is therefore about 1357 milliseconds under the SLH-DSA baseline and about 0.45 milliseconds under the ML-DSA candidate, a reduction of about three thousand times.

The projected finality floor follows directly. Under the SLH-DSA baseline the attestation cost alone is about nine times the 150 millisecond slot, so the prove overruns the slot and finality stays multi second whatever the hardware, which is the floor the main run already reports. Under the ML-DSA candidate the attestation cost is well under one percent of the slot, so the attestation is no longer the floor and sub second deterministic finality is reachable, bounded then by block propagation and certificate verification rather than by the prove. Every other term of the finality path is unchanged between the two constructions, so the change in the floor is exactly the change in the prove time.

This is the evidence for making the ML-DSA construction the default while keeping the SLH-DSA construction as the conservative baseline that allows unlimited evaluations. The compact proof wrapper the specification describes for the lattice construction shrinks the proof size rather than the prove time and is deferred, so it changes neither of these measured figures.

## Configuration and reproducibility

The stack crates are pinned by git tag in `Cargo.toml`. Cargo.lock is not committed. Run `cargo build --release`, `cargo fmt --check`, `cargo clippy --release`, and `cargo deny check` to reproduce the checks the benchmark is held to. The run prints the host profile, the topology matrix, and every measured figure with its spread, so a reader reconstructs the headline from the parts.
