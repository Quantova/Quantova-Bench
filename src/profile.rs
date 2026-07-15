//! The hardware profiles the benchmark holds the verifying side to.

/// A server class validator node profile. The figures are the host this
pub struct ServerProfile {
    pub name: &'static str,
    /// Cores used for parallel verification. Bounded to the host performance
    pub verify_cores: usize,
    /// Symmetric uplink modelled for a node, in bits per second.
    pub uplink_bps: u64,
    /// Working memory a node is assumed to hold, in gigabytes.
    pub memory_gb: u64,
}

impl ServerProfile {
    pub fn host() -> ServerProfile {
        ServerProfile {
            name:
                "Apple M4, 10 cores (4 performance + 6 efficiency), 16 GB, as a modest server node",
            verify_cores: 4,
            uplink_bps: 1_000_000_000,
            memory_gb: 16,
        }
    }

    /// Bytes per second the uplink carries.
    pub fn uplink_bytes_per_sec(&self) -> f64 {
        self.uplink_bps as f64 / 8.0
    }
}

/// A phone class profile, a 2020 reference phone. Used only to report whether a
pub struct PhoneProfile {
    pub name: &'static str,
    pub single_thread_derate: f64,
}

impl PhoneProfile {
    pub fn reference_2020() -> PhoneProfile {
        PhoneProfile {
            name: "2020 reference phone, 4 usable big cores",
            single_thread_derate: 3.0,
        }
    }
}
