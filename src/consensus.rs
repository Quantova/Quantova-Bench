// Copyright 2026 Quantova Inc
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A real QORUS stage one finality certificate, built from real committee

use qtv_attest::aggregate::aggregate;
use qtv_attest::params::finality_threshold;
use qtv_attest::{Attester, Block, CommitteeCommitment, Parent};
use qtv_sampler::beacon::Beacon;
use qtv_sampler::committee::{CommitteeView, PublishedReveal};
use qtv_sampler::params::COMMITTEE_BUDGET;
use qtv_sampler::validator::{Registration, SamplerValidator};

/// The wire size of one stage one attestation: the ML-DSA signature, the VRF
pub const PER_ATTESTATION_BYTES: usize = qtv_crypto::ml_dsa::SIGNATURE_BYTES
    + qtv_crypto::vrf::PROOF_BYTES
    + qtv_crypto::vrf::OUTPUT_BYTES
    + 24;

/// The wire size of one committee member entry a verifier must hold: the ML-DSA
pub const PER_MEMBER_KEY_BYTES: usize =
    qtv_crypto::ml_dsa::PUBLIC_KEY_BYTES + qtv_crypto::vrf::PUBLIC_KEY_BYTES;

/// A built real certificate, its committee commitment, and the beacon it was
pub struct RealCertificate {
    cert: qtv_attest::Certificate,
    commitment: CommitteeCommitment,
    beacon: Beacon,
    tau: u64,
    pub attesters: usize,
}

/// The 32-byte secret a benchmark committee member is built from. The v0.8.0
pub fn member_secret(id: u64) -> [u8; 32] {
    const DOMAIN: &[u8] = b"QORUS/TEST-ONLY/insecure-fixture-secret/v1";
    let mut buf = Vec::with_capacity(DOMAIN.len() + 8);
    buf.extend_from_slice(DOMAIN);
    buf.extend_from_slice(&id.to_le_bytes());
    let mut out = [0u8; 32];
    qtv_crypto::sha3::shake256(&buf, &mut out);
    out
}

impl RealCertificate {
    /// Build a real certificate over `header_hash` from a committee of about
    pub fn build(
        committee_real: usize,
        height: u64,
        slot: u64,
        header_hash: &[u8; 32],
    ) -> RealCertificate {
        let stake = qtv_bft::params::VALIDATOR_STAKE_QTOV;
        let sampler: Vec<SamplerValidator> = (0..committee_real as u64)
            .map(|id| SamplerValidator::from_secret(id + 1, &member_secret(id + 1), stake))
            .collect();
        let attesters: Vec<Attester> = (0..committee_real as u64)
            .map(|id| Attester::from_secret(id + 1, &member_secret(id + 1), stake))
            .collect();

        let beacon = Beacon::genesis();
        // Draw the committee the way a node does: form it from the reveals the
        // validators publish for the slot against a view over their registrations,
        // which reproduces the roster sampler's sortition draw member for member.
        let view = CommitteeView::new(sampler.iter().map(Registration::of).collect());
        let published: Vec<PublishedReveal> = sampler
            .iter()
            .map(|v| PublishedReveal::new(v.id, v.reveal(slot)))
            .collect();
        let committee = view.form_committee(&beacon, slot, &published);
        assert!(!committee.is_empty(), "the sortition admitted a committee");
        let member_ids = committee.ids();
        let refs: Vec<&Attester> = member_ids
            .iter()
            .filter_map(|id| attesters.iter().find(|a| a.id() == *id))
            .collect();
        let commitment =
            CommitteeCommitment::from_attesters_with_budget(slot, &refs, COMMITTEE_BUDGET);

        // The block value is the real block header hash. Now that a block carries a
        // full 32 byte value, the committee attests over the header directly, so the
        // certificate binds the block the workload produced.
        let block = Block::new(height, *header_hash, Parent::Genesis);
        // The quorum a certificate needs is the two thirds finality threshold of the
        // committee. A fully online committee clears it, and the verifier applies the
        // same threshold, so build and check share one canonical value.
        let tau = finality_threshold(member_ids.len() as u64);
        let attestations: Vec<_> = refs
            .iter()
            .map(|a| a.attest(height, slot, 0, block, &beacon))
            .collect();
        let cert = aggregate(height, slot, block, &commitment, &beacon, &attestations, tau)
            .expect("an online committee forms a quorum");
        let attesters = cert.attesters().len();
        RealCertificate {
            cert,
            commitment,
            beacon,
            tau,
            attesters,
        }
    }

    /// Verify the certificate against its commitment and beacon, the exact check
    pub fn verify(&self) -> bool {
        self.cert
            .verify(&self.commitment, &self.beacon, self.tau)
            .is_verified()
    }
}

/// The projected wire size of a stage one certificate for a committee of
pub fn certificate_bytes(attesters: usize) -> usize {
    attesters * PER_ATTESTATION_BYTES
}
