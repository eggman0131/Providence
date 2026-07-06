//! FNV-1a 64-bit hashing for state fingerprints.
//!
//! Used by the determinism/replay harness (ADR 0009) to fingerprint state
//! histories. Hand-rolled to keep the core zero-dependency (ADR 0006/0009);
//! the hash is stable across runs, platforms, and compiler versions.

/// Incremental FNV-1a 64-bit hasher.
#[derive(Debug, Clone)]
pub struct Fnv1a64 {
    state: u64,
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv1a64 {
    // FNV-1a algorithm constants — structural, not behavioural tuning values.
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325; // gate:allow(magic) FNV-1a offset basis
    const PRIME: u64 = 0x0000_0100_0000_01b3; // gate:allow(magic) FNV-1a prime

    /// New hasher, initialised at the FNV offset basis.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Self::OFFSET_BASIS,
        }
    }

    /// Absorb one `u64`, as little-endian bytes.
    pub fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.state ^= u64::from(byte);
            self.state = self.state.wrapping_mul(Self::PRIME);
        }
    }

    /// The current hash value.
    #[must_use]
    pub fn finish(&self) -> u64 {
        self.state
    }
}
