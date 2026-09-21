use lumbar::{Access, Associativity, CacheConfigBuilder, Hierarchy, Simulator};

const LINE_SIZE: usize = 64;
const ELEM_SIZE: u64 = 4; // f32

fn l1_8way_32kib() -> Hierarchy {
    let l1 = CacheConfigBuilder::new(
        32 * 1024,
        LINE_SIZE,
        Associativity::SetAssociative(8.try_into().unwrap()),
    )
    .build()
    .expect("valid cache config");
    Hierarchy::builder().add_cache("L1", l1).backing("mem", 0)
}

fn column_access(
    rows: usize,
    row_stride_elems: u64,
    passes: usize,
) -> impl Iterator<Item = Access> {
    (0..passes).flat_map(move |_| {
        (0..rows).map(move |r| Access::read(r as u64 * row_stride_elems * ELEM_SIZE, 4))
    })
}

/// A stride that is a multiple of `line_size * num_sets` (here 64 * 64 =
/// 4096 bytes = 1024 elements) makes every row alias the same cache set, so
/// a cache with plenty of raw capacity still thrashes; padding by one line
/// spreads rows across all sets and recovers a high hit rate.
#[test]
fn padding_stride_by_one_line_recovers_hit_rate() {
    let (rows, passes) = (64, 5);
    let bad_stride = 1024;
    let good_stride = bad_stride + (LINE_SIZE as u64 / ELEM_SIZE);

    let mut sim_bad = Simulator::new(l1_8way_32kib());
    let bad_rate = sim_bad
        .run(column_access(rows, bad_stride, passes))
        .per_level[0]
        .hit_rate();

    let mut sim_good = Simulator::new(l1_8way_32kib());
    let good_rate = sim_good
        .run(column_access(rows, good_stride, passes))
        .per_level[0]
        .hit_rate();

    assert!(
        bad_rate < 0.1,
        "expected the aliased stride to thrash: hit rate = {bad_rate}"
    );
    assert!(
        good_rate > 0.7,
        "expected the padded stride to mostly hit: hit rate = {good_rate}"
    );
}
