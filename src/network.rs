//! The global network model. Validators are globally distributed server nodes,

/// The regions the committee is spread across.
pub const REGIONS: [&str; 5] = ["N.America", "S.America", "Europe", "Asia", "Oceania"];

/// Symmetric one-way latency between regions in milliseconds. Intra-region is on
pub const LATENCY_MS: [[f64; 5]; 5] = [
    //         NA     SA     EU     AS     OC
    /* NA */ [5.0, 55.0, 40.0, 75.0, 70.0],
    /* SA */ [55.0, 5.0, 55.0, 160.0, 110.0],
    /* EU */ [40.0, 55.0, 5.0, 85.0, 130.0],
    /* AS */ [75.0, 160.0, 85.0, 5.0, 60.0],
    /* OC */ [70.0, 110.0, 130.0, 60.0, 5.0],
];

/// The gossip fanout, the number of peers a node forwards a fresh message to.
pub const GOSSIP_FANOUT: usize = 8;

/// The average one-way hop latency across the topology, weighting an intra-region
pub fn average_hop_latency_ms() -> f64 {
    let regions = REGIONS.len();
    let mut sum = 0.0;
    for row in LATENCY_MS.iter() {
        for &l in row.iter() {
            sum += l;
        }
    }
    // Every ordered region pair is equally likely under an even spread.
    sum / (regions * regions) as f64
}

/// The number of gossip hops to reach a committee of `nodes` under the fanout.
pub fn gossip_hops(nodes: usize) -> u32 {
    if nodes <= 1 {
        return 0;
    }
    (nodes as f64).log(GOSSIP_FANOUT as f64).ceil() as u32
}

/// Milliseconds to propagate a payload of `bytes` to a committee of `nodes` over
pub fn propagate_ms(bytes: usize, nodes: usize, uplink_bytes_per_sec: f64) -> f64 {
    let hops = gossip_hops(nodes) as f64;
    let transmit_ms = (bytes as f64 / uplink_bytes_per_sec) * 1_000.0;
    hops * (average_hop_latency_ms() + transmit_ms)
}

/// The measured compute terms of the finality path, in nanoseconds.
pub struct ComputeCosts {
    pub build_ns: f64,
    pub verify_block_ns: f64,
    pub attest_ns: f64,
    pub aggregate_ns: f64,
    pub verify_cert_ns: f64,
}

/// The payload sizes of the finality path, in bytes.
pub struct Sizes {
    pub block_bytes: usize,
    pub certificate_bytes: usize,
}

/// The finality path broken into its terms, all in milliseconds.
pub struct Finality {
    pub build: f64,
    pub propagate_block: f64,
    pub verify_block: f64,
    pub attest: f64,
    pub disseminate_attestations: f64,
    pub aggregate: f64,
    pub verify_certificate: f64,
}

impl Finality {
    /// Assemble the finality path for a committee of `nodes` from the measured
    pub fn assemble(
        compute: &ComputeCosts,
        sizes: &Sizes,
        nodes: usize,
        uplink_bytes_per_sec: f64,
    ) -> Finality {
        let to_ms = |ns: f64| ns / 1_000_000.0;
        Finality {
            build: to_ms(compute.build_ns),
            propagate_block: propagate_ms(sizes.block_bytes, nodes, uplink_bytes_per_sec),
            verify_block: to_ms(compute.verify_block_ns),
            attest: to_ms(compute.attest_ns),
            disseminate_attestations: propagate_ms(
                sizes.certificate_bytes,
                nodes,
                uplink_bytes_per_sec,
            ),
            aggregate: to_ms(compute.aggregate_ns),
            verify_certificate: to_ms(compute.verify_cert_ns),
        }
    }

    /// The total finality latency in milliseconds.
    pub fn total_ms(&self) -> f64 {
        self.build
            + self.propagate_block
            + self.verify_block
            + self.attest
            + self.disseminate_attestations
            + self.aggregate
            + self.verify_certificate
    }
}
