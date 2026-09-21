//! A small victim cache -- a tiny, fully-associative buffer catching a
//! primary cache's evictions -- is a classic cheap fix for conflict misses
//! in a low-associativity cache, without redesigning its associativity.
//!
//! Run with: `cargo run --example victim_cache`

use lumbar::{Access, Associativity, CacheConfigBuilder, Hierarchy, Simulator};

const LINE_SIZE: usize = 64;

/// `rows` addresses that all alias the same set of an 8-way, 32KiB L1 (same
/// trick as the power-of-two-stride example): more rows than ways means the
/// extra ones constantly evict each other on repeated passes.
fn aliased_access(rows: usize, passes: usize) -> impl Iterator<Item = Access> {
    let stride = 1024u64; // elements; see power_of_two_stride.rs for why
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

fn main() {
    // 12 rows contending for one 8-way set: 4 lines' worth of overflow, so a
    // 4-entry victim cache can hold exactly the surplus.
    let (rows, passes, victim_lines) = (12, 5, 4);

    for (label, victim) in [
        ("no victim cache", 0),
        ("4-line victim cache", victim_lines),
    ] {
        let mut sim = Simulator::new(l1(victim));
        let result = sim.run(aliased_access(rows, passes));
        let l1 = &result.per_level[0];
        println!("{label:>20}: hit rate = {:5.1}%", l1.hit_rate() * 100.0);
    }
}
