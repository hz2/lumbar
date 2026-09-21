use lumbar::{
    Access, Associativity, CacheConfigBuilder, Hierarchy, ReplacementPolicy, Simulator,
    belady_optimal,
};

fn cache(
    size_bytes: usize,
    associativity: Associativity,
    policy: ReplacementPolicy,
) -> lumbar::CacheConfig {
    CacheConfigBuilder::new(size_bytes, 64, associativity)
        .replacement_policy(policy)
        .build()
        .unwrap()
}

fn line_access(block: u64) -> Access {
    Access::read(block * 64, 4)
}

#[test]
fn belady_beats_lru_on_a_sequence_lru_handles_badly() {
    // LRU thrashes on a cyclic scan slightly larger than the cache (every
    // access is a miss, since the item it needs is always the one LRU just
    // evicted); Belady can see the repeat coming and keeps the right lines.
    let blocks: Vec<u64> = (0..5).cycle().take(20).collect();
    let trace: Vec<Access> = blocks.iter().map(|&b| line_access(b)).collect();

    let lru_cache = cache(
        4 * 64,
        Associativity::FullyAssociative,
        ReplacementPolicy::Lru,
    );
    let mut sim = Simulator::new(
        Hierarchy::builder()
            .add_cache("L1", lru_cache)
            .backing("mem", 0),
    );
    let lru_rate = sim.run(trace.clone()).per_level[0].hit_rate();

    let belady_cache = cache(
        4 * 64,
        Associativity::FullyAssociative,
        ReplacementPolicy::Lru,
    );
    let belady_stats = belady_optimal(&belady_cache, &trace);

    assert!(
        belady_stats.hit_rate() >= lru_rate,
        "belady={} lru={lru_rate}",
        belady_stats.hit_rate()
    );
}

proptest::proptest! {
    /// Belady's MIN is provably optimal, so on any trace and any fixed cache
    /// shape, it must never do worse than what an online policy achieves.
    #[test]
    fn belady_never_loses_to_lru_or_fifo(
        blocks in proptest::collection::vec(0u64..12, 0..200),
        ways in 1usize..8,
        policy in proptest::sample::select(vec![ReplacementPolicy::Lru, ReplacementPolicy::Fifo]),
    ) {
        let trace: Vec<Access> = blocks.iter().map(|&b| line_access(b)).collect();
        // one set, `ways` wide: num_lines == ways, so it always divides evenly
        let associativity = Associativity::SetAssociative(ways.try_into().unwrap());
        let size_bytes = ways * 64;

        let online_cache = cache(size_bytes, associativity, policy);
        let mut sim = Simulator::new(Hierarchy::builder().add_cache("L1", online_cache).backing("mem", 0));
        let online_rate = sim.run(trace.clone()).per_level[0].hit_rate();

        let belady_cache = cache(size_bytes, associativity, ReplacementPolicy::Lru);
        let belady_rate = belady_optimal(&belady_cache, &trace).hit_rate();

        proptest::prop_assert!(belady_rate >= online_rate - 1e-9);
    }
}
