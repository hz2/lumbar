use std::num::NonZeroUsize;

use lumbar::{
    Access, Associativity, CacheConfigBuilder, Hierarchy, ReplacementPolicy, Simulator,
    reuse_distances,
};

const LINE_SIZE: usize = 8;

/// A deliberately naive single-level fully-associative LRU cache, written
/// independently of [`Simulator`]'s internals, to differentially test against.
struct ReferenceLru {
    capacity_lines: usize,
    /// Resident block addresses, front = most recently used.
    resident: Vec<u64>,
}

impl ReferenceLru {
    fn new(capacity_lines: usize) -> Self {
        Self {
            capacity_lines,
            resident: Vec::new(),
        }
    }

    /// Returns `true` if `block` was already resident (a hit).
    fn access(&mut self, block: u64) -> bool {
        if let Some(pos) = self.resident.iter().position(|&b| b == block) {
            self.resident.remove(pos);
            self.resident.insert(0, block);
            return true;
        }
        self.resident.insert(0, block);
        if self.resident.len() > self.capacity_lines {
            self.resident.pop();
        }
        false
    }
}

fn fully_associative_lru(capacity_lines: usize) -> Hierarchy {
    let cache = CacheConfigBuilder::new(
        capacity_lines * LINE_SIZE,
        LINE_SIZE,
        Associativity::FullyAssociative,
    )
    .replacement_policy(ReplacementPolicy::Lru)
    .build()
    .expect("valid cache config");
    Hierarchy::builder()
        .add_cache("L1", cache)
        .backing("mem", 0)
}

fn line_access(block: u64) -> Access {
    Access::read(block * LINE_SIZE as u64, LINE_SIZE as u32)
}

fn check_matches_reference(blocks: &[u64], capacity_lines: usize) {
    let mut reference = ReferenceLru::new(capacity_lines);
    let expected_hits = blocks.iter().filter(|&&b| reference.access(b)).count() as u64;

    let mut sim = Simulator::new(fully_associative_lru(capacity_lines));
    let result = sim.run(blocks.iter().map(|&b| line_access(b)));
    let l1 = &result.per_level[0];
    assert_eq!(l1.hits, expected_hits, "capacity={capacity_lines}");
    assert_eq!(
        l1.misses,
        blocks.len() as u64 - expected_hits,
        "capacity={capacity_lines}"
    );
}

#[test]
fn differential_against_reference_lru_fixed_trace() {
    let blocks = [1u64, 2, 3, 1, 4, 1, 2, 5, 1, 2, 3, 4, 5];
    for capacity in [1, 2, 3, 5, 100] {
        check_matches_reference(&blocks, capacity);
    }
}

// hand-checkable textbook example: 4 sets, direct-mapped. Blocks 0 and 4 both
// map to set 0, so each access evicts the other and every access misses.
#[test]
fn direct_mapped_conflict_misses() {
    let cache = CacheConfigBuilder::new(4 * LINE_SIZE, LINE_SIZE, Associativity::DirectMapped)
        .build()
        .expect("valid cache config");
    let mut sim = Simulator::new(
        Hierarchy::builder()
            .add_cache("L1", cache)
            .backing("mem", 0),
    );

    let blocks = [0u64, 4, 0, 4, 0, 4];
    let result = sim.run(blocks.iter().map(|&b| line_access(b)));
    assert_eq!(result.per_level[0].hits, 0);
    assert_eq!(result.per_level[0].misses, 6);
}

// same blocks, but 2-way associative: both blocks map to the same set and now
// fit together, so only the first touch of each is a miss.
#[test]
fn two_way_associative_avoids_the_conflict() {
    let ways = NonZeroUsize::new(2).expect("2 != 0");
    let cache = CacheConfigBuilder::new(
        4 * LINE_SIZE,
        LINE_SIZE,
        Associativity::SetAssociative(ways),
    )
    .build()
    .expect("valid cache config");
    let mut sim = Simulator::new(
        Hierarchy::builder()
            .add_cache("L1", cache)
            .backing("mem", 0),
    );

    let blocks = [0u64, 4, 0, 4, 0, 4];
    let result = sim.run(blocks.iter().map(|&b| line_access(b)));
    assert_eq!(result.per_level[0].misses, 2);
    assert_eq!(result.per_level[0].hits, 4);
}

/// Cross-checks two algorithmically unrelated implementations against each
/// other: the explicit hierarchy walk in [`Simulator`], and the single-pass
/// LRU stack-distance algorithm in [`reuse_distances`].
#[test]
fn reuse_distance_histogram_matches_fully_associative_simulator() {
    let blocks = [1u64, 2, 3, 1, 4, 1, 2, 5, 1, 2, 3, 4, 5, 6, 1, 7, 2];
    let histogram = reuse_distances(blocks.iter().copied());

    for capacity in [1usize, 2, 3, 4, 8] {
        let mut sim = Simulator::new(fully_associative_lru(capacity));
        let result = sim.run(blocks.iter().map(|&b| line_access(b)));
        let sim_hit_rate = result.per_level[0].hit_rate();
        let histogram_hit_rate = histogram.hit_rate_at_capacity(capacity);
        assert!(
            (sim_hit_rate - histogram_hit_rate).abs() < 1e-9,
            "capacity={capacity} sim={sim_hit_rate} histogram={histogram_hit_rate}"
        );
    }
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn hits_plus_misses_equals_trace_length(
            blocks in proptest::collection::vec(0u64..12, 0..200),
            capacity in 1usize..8,
        ) {
            let mut sim = Simulator::new(fully_associative_lru(capacity));
            let len = blocks.len() as u64;
            let result = sim.run(blocks.iter().map(|&b| line_access(b)));
            prop_assert_eq!(result.per_level[0].accesses(), len);
        }

        #[test]
        fn hit_rate_is_a_fraction(
            blocks in proptest::collection::vec(0u64..12, 0..200),
            capacity in 1usize..8,
        ) {
            let mut sim = Simulator::new(fully_associative_lru(capacity));
            let result = sim.run(blocks.iter().map(|&b| line_access(b)));
            let rate = result.per_level[0].hit_rate();
            prop_assert!((0.0..=1.0).contains(&rate));
        }

        /// The LRU "stack property": for a fixed trace, a larger fully
        /// associative cache never has a lower hit rate than a smaller one.
        #[test]
        fn larger_fully_associative_lru_cache_never_loses_hit_rate(
            blocks in proptest::collection::vec(0u64..16, 0..200),
            small in 1usize..6,
            grow in 1usize..6,
        ) {
            let large = small + grow;
            let mut sim_small = Simulator::new(fully_associative_lru(small));
            let mut sim_large = Simulator::new(fully_associative_lru(large));
            let small_rate = sim_small.run(blocks.iter().map(|&b| line_access(b))).per_level[0].hit_rate();
            let large_rate = sim_large.run(blocks.iter().map(|&b| line_access(b))).per_level[0].hit_rate();
            prop_assert!(large_rate >= small_rate - 1e-9);
        }
    }
}
