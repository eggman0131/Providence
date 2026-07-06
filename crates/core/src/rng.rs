//! Seeded, deterministic randomness for the core (invariant I3).
//!
//! `SplitMix64`: tiny, well-understood, bit-for-bit reproducible. Hand-rolled
//! to keep the core zero-dependency (ADR 0006/0009). Streams are created at
//! the edge (`ClockRandomPort`, Phase 4) and passed inward; the core never
//! owns a global RNG.

/// A `SplitMix64` pseudo-random stream. Same seed ⇒ same stream, forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Create a stream from a seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Produce the next value in the stream.
    pub fn next_u64(&mut self) -> u64 {
        // SplitMix64 algorithm constants (Steele, Lea & Flood 2014) —
        // structural, not behavioural tuning values.
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15); // gate:allow(magic) SplitMix64 gamma
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9); // gate:allow(magic) SplitMix64 mix constant
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb); // gate:allow(magic) SplitMix64 mix constant
        z ^ (z >> 31) // gate:allow(magic) SplitMix64 finaliser shift
    }
}
