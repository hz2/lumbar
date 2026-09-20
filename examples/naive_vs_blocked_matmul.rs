//! Shows lumbar's core value proposition: simulating naive vs. blocked matmul
//! against the same cache reveals why blocking helps, without touching real
//! hardware.
//!
//! Run with: `cargo run --example naive_vs_blocked_matmul`

use lumbar::patterns::{matmul_blocked, matmul_naive};
use lumbar::{Associativity, CacheConfigBuilder, Hierarchy, Simulator};

fn l1_like_hierarchy() -> Hierarchy {
    let l1 = CacheConfigBuilder::new(
        32 * 1024,
        64,
        Associativity::SetAssociative(8.try_into().unwrap()),
    )
    .latency_cycles(4)
    .build()
    .expect("valid cache config");
    Hierarchy::builder()
        .add_cache("L1", l1)
        .backing("DRAM", 200)
}

fn main() {
    let (m, n, k) = (256, 256, 256);
    let block = 32;
    let elem_size = 4; // f32

    let mut sim_naive = Simulator::new(l1_like_hierarchy());
    let naive = sim_naive.run(matmul_naive(m, n, k, elem_size));

    let mut sim_blocked = Simulator::new(l1_like_hierarchy());
    let blocked = sim_blocked.run(matmul_blocked(m, n, k, block, elem_size));

    println!("{m}x{n}x{k} f32 matmul, 32KiB 8-way L1, 64B lines, block={block}\n");
    for (label, result) in [("naive", &naive), ("blocked", &blocked)] {
        let l1 = &result.per_level[0];
        println!(
            "{label:>7}: L1 hit rate = {:5.1}%  AMAT = {:.2} cycles",
            l1.hit_rate() * 100.0,
            result.amat_cycles
        );
    }
}
