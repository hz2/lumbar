//! Lumbar simulates and analyzes memory-access patterns against a
//! configurable, hardware-agnostic cache/memory hierarchy, without needing
//! real hardware.
//!
//! - [`Simulator`] walks a trace of [`Access`]es through a [`Hierarchy`],
//!   producing per-level hit/miss counts and an AMAT estimate.
//! - [`reuse_distances`] computes idealized fully-associative-LRU hit rates
//!   for every cache size at once from a single pass over an address stream.
//! - [`advise_working_set`] gives a hedged capacity check against a
//!   hierarchy's level sizes.
//! - [`patterns`] has built-in generators (naive/blocked matmul, row/column
//!   major, transpose, stencil) for trying the above without hand-writing a
//!   trace.
//! - [`belady_optimal`] simulates the provably optimal offline replacement
//!   policy against a single cache level, as a ceiling to compare an online
//!   policy like LRU against.
//!
//! ```
//! use lumbar::{Access, Associativity, CacheConfigBuilder, Hierarchy, Simulator};
//!
//! // 1KiB direct-mapped cache, 64-byte lines: exactly 16 lines
//! let cache = CacheConfigBuilder::new(1024, 64, Associativity::DirectMapped).build().unwrap();
//! let hierarchy = Hierarchy::builder().add_cache("L1", cache).backing("mem", 0);
//!
//! // touch all 16 lines once, then repeat: first pass is all cold misses,
//! // second pass hits since nothing evicted them
//! let trace = (0..2).flat_map(|_| (0..16).map(|line| Access::read(line * 64, 4)));
//! let result = Simulator::new(hierarchy).run(trace);
//! assert_eq!(result.per_level[0].misses, 16);
//! assert_eq!(result.per_level[0].hits, 16);
//! ```

mod access;
mod advisory;
mod belady;
mod hierarchy;
#[cfg(feature = "patterns")]
pub mod patterns;
mod reuse;
mod simulator;

pub use access::{Access, AccessKind, Address, StreamId};
pub use advisory::{WorkingSetAdvisory, advise_working_set};
pub use belady::belady_optimal;
pub use hierarchy::cache_config::{
    Associativity, CacheConfig, CacheConfigBuilder, CacheConfigError, InclusionPolicy, WritePolicy,
};
pub use hierarchy::replacement::ReplacementPolicy;
pub use hierarchy::{Hierarchy, HierarchyBuilder, Level, LevelKind};
pub use reuse::{ReuseDistanceHistogram, reuse_distances};
pub use simulator::{LevelStats, SimulationResult, Simulator};
