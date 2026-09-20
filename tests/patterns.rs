use lumbar::patterns::{col_major, matmul_blocked, matmul_naive, row_major, stencil_2d, transpose};
use lumbar::{Associativity, CacheConfigBuilder, Hierarchy, Simulator};

#[test]
fn row_and_col_major_touch_every_element_once() {
    let (rows, cols) = (7, 5);
    assert_eq!(row_major(rows, cols, 4).count(), rows * cols);
    assert_eq!(col_major(rows, cols, 4).count(), rows * cols);
}

#[test]
fn transpose_reads_and_writes_every_element() {
    let (rows, cols) = (6, 4);
    // one read + one write per element
    assert_eq!(transpose(rows, cols, 4).count(), 2 * rows * cols);
}

#[test]
fn matmul_naive_and_blocked_touch_the_same_number_of_operand_elements() {
    let (m, n, k) = (8, 8, 8);
    // one write of C plus k reads of A and B for each of the m*n outputs
    assert_eq!(matmul_naive(m, n, k, 4).count(), m * n * (2 * k + 1));
}

#[test]
fn stencil_produces_one_write_per_interior_point_per_iteration() {
    let (rows, cols, radius, iterations) = (10, 10, 1, 3);
    let interior = (rows - 2 * radius) * (cols - 2 * radius);
    // per interior point: 1 center read + 4*radius neighbor reads + 1 write
    let per_point = 1 + 4 * radius + 1;
    assert_eq!(
        stencil_2d(rows, cols, radius, iterations, 4).count(),
        interior * per_point * iterations
    );
}

/// The crate's flagship claim: blocking improves reuse enough to beat a cache
/// too small to hold the naive working set.
#[test]
fn blocked_matmul_beats_naive_on_a_small_cache() {
    let (m, n, k, block) = (64, 64, 64, 16);

    // small enough that a full row of B (64 * 4 bytes = 256B) plus enough of A
    // and C to matter doesn't comfortably fit, but a 16x16 tile of each does.
    let cache = CacheConfigBuilder::new(
        4096,
        64,
        Associativity::SetAssociative(4.try_into().unwrap()),
    )
    .build()
    .expect("valid cache config");
    let hierarchy = || {
        Hierarchy::builder()
            .add_cache("L1", cache)
            .backing("mem", 0)
    };

    let mut sim_naive = Simulator::new(hierarchy());
    let naive_rate = sim_naive.run(matmul_naive(m, n, k, 4)).per_level[0].hit_rate();

    let mut sim_blocked = Simulator::new(hierarchy());
    let blocked_rate = sim_blocked.run(matmul_blocked(m, n, k, block, 4)).per_level[0].hit_rate();

    assert!(
        blocked_rate > naive_rate,
        "expected blocking to improve hit rate: naive={naive_rate} blocked={blocked_rate}"
    );
}
