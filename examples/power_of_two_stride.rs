//! The classic "power-of-two stride" cache pathology: real caches pick a
//! line's set from a fixed slice of the address's own bits, so rows spaced a
//! multiple of `line_size * num_sets` apart all alias onto the same set. A
//! cache with easily enough *capacity* for all of them still thrashes,
//! because they're all fighting over that one set's `ways` slots. Padding
//! the stride by a single cache line breaks the alignment and fixes it, with
//! no other change to the access pattern.
//!
//! Run with: `cargo run --example power_of_two_stride`

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

/// Reads column 0 of `rows` rows spaced `row_stride_elems` elements apart,
/// `passes` times over.
fn column_access(
    rows: usize,
    row_stride_elems: u64,
    passes: usize,
) -> impl Iterator<Item = Access> {
    (0..passes).flat_map(move |_| {
        (0..rows).map(move |r| Access::read(r as u64 * row_stride_elems * ELEM_SIZE, 4))
    })
}

fn main() {
    let rows = 64;
    let passes = 5;

    // 1024 elements/row * 4 bytes = 4096 bytes = exactly line_size(64) *
    // num_sets(64): every row's column-0 element lands in the same set.
    let bad_stride = 1024;
    // padding by one extra 64-byte line (16 elements) breaks the alignment,
    // spreading rows evenly across all 64 sets instead.
    let good_stride = bad_stride + 16;

    println!("{rows} rows, {passes} passes, 32KiB 8-way L1, 64B lines (64 sets)\n");
    for (label, stride) in [
        ("power-of-two stride", bad_stride),
        ("padded stride", good_stride),
    ] {
        let mut sim = Simulator::new(l1_8way_32kib());
        let result = sim.run(column_access(rows, stride, passes));
        let l1 = &result.per_level[0];
        println!(
            "{label:>20} (stride={stride:>5} elems): hit rate = {:5.1}%",
            l1.hit_rate() * 100.0
        );
    }
}
