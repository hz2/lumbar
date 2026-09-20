use std::num::NonZeroUsize;

use super::replacement::ReplacementPolicy;

/// How lines map to sets within a cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Associativity {
    DirectMapped,
    SetAssociative(NonZeroUsize),
    FullyAssociative,
}

/// Whether a write reaches the next level immediately or only on eviction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePolicy {
    WriteThrough,
    WriteBack,
}

/// How a level's contents relate to the levels behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InclusionPolicy {
    NonInclusive,
    Inclusive,
    Exclusive,
}

/// An invalid combination of size, line size, and associativity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheConfigError {
    ZeroSize,
    ZeroLineSize,
    SizeNotMultipleOfLineSize,
    WaysDoNotDivideLineCount,
}

impl std::fmt::Display for CacheConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::ZeroSize => "cache size must be greater than zero",
            Self::ZeroLineSize => "line size must be greater than zero",
            Self::SizeNotMultipleOfLineSize => "cache size must be a multiple of the line size",
            Self::WaysDoNotDivideLineCount => "the number of ways must divide the number of lines",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for CacheConfigError {}

/// A validated configuration for one cache level.
///
/// Constructed via [`CacheConfigBuilder`], which checks that `size_bytes`,
/// `line_size_bytes`, and the chosen [`Associativity`] are mutually
/// consistent before a `CacheConfig` can exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheConfig {
    size_bytes: usize,
    line_size_bytes: usize,
    ways: usize,
    num_sets: usize,
    associativity: Associativity,
    replacement_policy: ReplacementPolicy,
    write_policy: WritePolicy,
    write_allocate: bool,
    inclusion: InclusionPolicy,
    latency_cycles: u32,
}

impl CacheConfig {
    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }

    pub fn line_size_bytes(&self) -> usize {
        self.line_size_bytes
    }

    /// Number of ways per set (1 for direct-mapped, `size / line_size` for
    /// fully associative).
    pub fn ways(&self) -> usize {
        self.ways
    }

    pub fn num_sets(&self) -> usize {
        self.num_sets
    }

    pub fn associativity(&self) -> Associativity {
        self.associativity
    }

    pub fn replacement_policy(&self) -> ReplacementPolicy {
        self.replacement_policy
    }

    pub fn write_policy(&self) -> WritePolicy {
        self.write_policy
    }

    pub fn write_allocate(&self) -> bool {
        self.write_allocate
    }

    pub fn inclusion(&self) -> InclusionPolicy {
        self.inclusion
    }

    pub fn latency_cycles(&self) -> u32 {
        self.latency_cycles
    }
}

/// Builds a [`CacheConfig`].
///
/// Defaults: [`ReplacementPolicy::Lru`], [`WritePolicy::WriteBack`] with
/// write-allocate, [`InclusionPolicy::NonInclusive`], zero latency. Only the
/// write-back plus write-allocate combination is exercised heavily by this
/// crate's own tests; the other write-policy combinations are implemented but
/// lightly tested.
#[derive(Clone, Copy, Debug)]
pub struct CacheConfigBuilder {
    size_bytes: usize,
    line_size_bytes: usize,
    associativity: Associativity,
    replacement_policy: ReplacementPolicy,
    write_policy: WritePolicy,
    write_allocate: bool,
    inclusion: InclusionPolicy,
    latency_cycles: u32,
}

impl CacheConfigBuilder {
    pub fn new(size_bytes: usize, line_size_bytes: usize, associativity: Associativity) -> Self {
        Self {
            size_bytes,
            line_size_bytes,
            associativity,
            replacement_policy: ReplacementPolicy::default(),
            write_policy: WritePolicy::WriteBack,
            write_allocate: true,
            inclusion: InclusionPolicy::NonInclusive,
            latency_cycles: 0,
        }
    }

    #[must_use]
    pub fn replacement_policy(mut self, policy: ReplacementPolicy) -> Self {
        self.replacement_policy = policy;
        self
    }

    #[must_use]
    pub fn write_policy(mut self, policy: WritePolicy) -> Self {
        self.write_policy = policy;
        self
    }

    #[must_use]
    pub fn write_allocate(mut self, write_allocate: bool) -> Self {
        self.write_allocate = write_allocate;
        self
    }

    #[must_use]
    pub fn inclusion(mut self, inclusion: InclusionPolicy) -> Self {
        self.inclusion = inclusion;
        self
    }

    #[must_use]
    pub fn latency_cycles(mut self, latency_cycles: u32) -> Self {
        self.latency_cycles = latency_cycles;
        self
    }

    /// Validates the configuration, computing `ways` and `num_sets`.
    ///
    /// ```
    /// use lumbar::{Associativity, CacheConfigBuilder, CacheConfigError};
    ///
    /// let cache = CacheConfigBuilder::new(32 * 1024, 64, Associativity::SetAssociative(8.try_into().unwrap()))
    ///     .build()
    ///     .unwrap();
    /// assert_eq!(cache.ways(), 8);
    /// assert_eq!(cache.num_sets(), 64); // 32KiB / 64B lines / 8 ways
    ///
    /// let invalid = CacheConfigBuilder::new(100, 64, Associativity::DirectMapped).build();
    /// assert_eq!(invalid, Err(CacheConfigError::SizeNotMultipleOfLineSize));
    /// ```
    pub fn build(self) -> Result<CacheConfig, CacheConfigError> {
        if self.size_bytes == 0 {
            return Err(CacheConfigError::ZeroSize);
        }
        if self.line_size_bytes == 0 {
            return Err(CacheConfigError::ZeroLineSize);
        }
        if !self.size_bytes.is_multiple_of(self.line_size_bytes) {
            return Err(CacheConfigError::SizeNotMultipleOfLineSize);
        }
        let num_lines = self.size_bytes / self.line_size_bytes;
        let ways = match self.associativity {
            Associativity::DirectMapped => 1,
            Associativity::FullyAssociative => num_lines,
            Associativity::SetAssociative(n) => n.get(),
        };
        if !num_lines.is_multiple_of(ways) {
            return Err(CacheConfigError::WaysDoNotDivideLineCount);
        }
        Ok(CacheConfig {
            size_bytes: self.size_bytes,
            line_size_bytes: self.line_size_bytes,
            ways,
            num_sets: num_lines / ways,
            associativity: self.associativity,
            replacement_policy: self.replacement_policy,
            write_policy: self.write_policy,
            write_allocate: self.write_allocate,
            inclusion: self.inclusion,
            latency_cycles: self.latency_cycles,
        })
    }
}
