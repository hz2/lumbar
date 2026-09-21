use lumbar::{Access, Associativity, CacheConfigBuilder, Hierarchy, Simulator};

const LINE_SIZE: usize = 64;

fn aliased_access(rows: usize, passes: usize) -> impl Iterator<Item = Access> {
    let stride = 1024u64;
    (0..passes).flat_map(move |_| (0..rows).map(move |r| Access::read(r as u64 * stride * 4, 4)))
}

fn l1(victim_lines: usize) -> Hierarchy {
    let mut builder = CacheConfigBuilder::new(
        32 * 1024,
        LINE_SIZE,
        Associativity::SetAssociative(8.try_into().unwrap()),
    );
    if victim_lines > 0 {
        builder = builder.victim_cache(victim_lines);
    }
    let l1 = builder.build().expect("valid cache config");
    Hierarchy::builder().add_cache("L1", l1).backing("mem", 0)
}

/// 12 addresses aliasing one 8-way set overflow by exactly 4 lines; a
/// 4-entry victim cache should absorb that overflow and recover most of the
/// hit rate an 8-way set alone loses to the conflict.
#[test]
fn victim_cache_recovers_hit_rate_lost_to_set_conflicts() {
    let (rows, passes) = (12, 5);

    let mut sim_no_victim = Simulator::new(l1(0));
    let no_victim_rate = sim_no_victim.run(aliased_access(rows, passes)).per_level[0].hit_rate();

    let mut sim_with_victim = Simulator::new(l1(4));
    let with_victim_rate =
        sim_with_victim.run(aliased_access(rows, passes)).per_level[0].hit_rate();

    assert!(
        no_victim_rate < 0.2,
        "expected heavy thrashing without a victim cache: {no_victim_rate}"
    );
    assert!(
        with_victim_rate > 0.7,
        "expected the victim cache to recover most hits: {with_victim_rate}"
    );
    assert!(with_victim_rate > no_victim_rate);
}

/// On this specific aliasing workload, growing the victim cache never loses
/// hit rate (not a claimed universal property of victim caches in general --
/// just checked here across a range of capacities on one trace).
#[test]
fn victim_cache_hit_rate_is_monotonic_in_capacity_for_this_trace() {
    for victim_lines in [0, 1, 2, 4, 8, 16] {
        let mut baseline = Simulator::new(l1(0));
        let baseline_rate = baseline.run(aliased_access(12, 5)).per_level[0].hit_rate();

        let mut with_victim = Simulator::new(l1(victim_lines));
        let victim_rate = with_victim.run(aliased_access(12, 5)).per_level[0].hit_rate();

        assert!(
            victim_rate >= baseline_rate,
            "victim_lines={victim_lines}: victim={victim_rate} baseline={baseline_rate}"
        );
    }
}
