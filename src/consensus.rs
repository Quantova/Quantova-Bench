// Copyright 2026 Quantova Inc
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A real QORUS stage one finality certificate, built from real committee

use qtv_attest::aggregate::aggregate;
use qtv_attest::{Attester, Block, CommitteeCommitment, Parent};
use qtv_sampler::beacon::Beacon;
use qtv_sampler::committee::Registry;
use qtv_sampler::params::COMMITTEE_BUDGET;
use qtv_sampler::validator::SamplerValidator;

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
    pub attesters: usize,
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
            .map(|id| SamplerValidator::new(id + 1, stake))
            .collect();
        let registry = Registry::new(sampler);
        let attesters: Vec<Attester> = (0..committee_real as u64)
            .map(|id| Attester::new(id + 1, stake))
            .collect();

        let beacon = Beacon::genesis();
        let committee = registry.sample_committee(&beacon, slot);
        assert!(!committee.is_empty(), "the sortition admitted a committee");
        let member_ids = committee.ids();
        let refs: Vec<&Attester> = member_ids
            .iter()
            .filter_map(|id| attesters.iter().find(|a| a.id() == *id))
            .collect();
        let commitment =
            CommitteeCommitment::from_attesters_with_budget(slot, &refs, COMMITTEE_BUDGET);

        let val = qtv_bft::hash::digest_u64(header_hash);
        let block = Block::new(height, val, Parent::Genesis);
        let attestations: Vec<_> = refs
            .iter()
            .map(|a| a.attest(height, slot, block, &beacon))
            .collect();
        let cert = aggregate(height, slot, block, &commitment, &beacon, &attestations)
            .expect("an online committee forms a quorum");
        let attesters = cert.attesters().len();
        RealCertificate {
            cert,
            commitment,
            beacon,
            attesters,
        }
    }

    /// Verify the certificate against its commitment and beacon, the exact check
    pub fn verify(&self) -> bool {
        self.cert
            .verify(&self.commitment, &self.beacon)
            .is_verified()
    }
}

/// The projected wire size of a stage one certificate for a committee of
pub fn certificate_bytes(attesters: usize) -> usize {
    attesters * PER_ATTESTATION_BYTES
}
