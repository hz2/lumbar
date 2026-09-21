# lumbar

[![crates.io](https://img.shields.io/crates/v/lumbar.svg)](https://crates.io/crates/lumbar)
[![CI](https://github.com/hz2/lumbar/actions/workflows/ci.yml/badge.svg)](https://github.com/hz2/lumbar/actions/workflows/ci.yml)

A hardware-agnostic cache and memory-hierarchy simulator for Rust. It lets
you reason about cache behavior for a given memory-access pattern before
touching real hardware: hit/miss rates, AMAT, reuse-distance curves, and a
working-set-fits-in-cache advisory.

```rust
use lumbar::patterns::{matmul_blocked, matmul_naive};
use lumbar::{Associativity, CacheConfigBuilder, Hierarchy, Simulator};

let hierarchy = || {
    let l1 = CacheConfigBuilder::new(32 * 1024, 64, Associativity::SetAssociative(8.try_into().unwrap()))
        .build()
        .unwrap();
    Hierarchy::builder().add_cache("L1", l1).backing("DRAM", 200)
};

let naive = Simulator::new(hierarchy()).run(matmul_naive(256, 256, 256, 4));
let blocked = Simulator::new(hierarchy()).run(matmul_blocked(256, 256, 256, 32, 4));

println!("naive:   {:.1}%", naive.per_level[0].hit_rate() * 100.0);
println!("blocked: {:.1}%", blocked.per_level[0].hit_rate() * 100.0);
```

## Examples

- `cargo run --example naive_vs_blocked_matmul` -- the walkthrough above, end
  to end.
- `cargo run --example power_of_two_stride` -- a cache with plenty of raw
  capacity can still thrash if every row of an access pattern lands in the
  same set; padding the stride by one cache line fixes it with no other
  change.

## What it does

- **`Simulator`** walks a stream of typed memory accesses through a
  configurable, multi-level `Hierarchy` (size, line size, associativity,
  replacement policy, write policy), producing per-level hit/miss counts and
  an AMAT estimate.
- **`reuse_distances`** computes LRU stack distances from a single pass over
  an address stream, giving hit-rate-vs-cache-size curves for the idealized
  fully-associative-LRU case without re-simulating per size.
- **`advise_working_set`** is a hedged, advisory-only check of whether a
  working set fits within each level of a hierarchy by size alone.
- **`patterns`** (on by default) has built-in generators for the classic
  locality demonstrations: row/column-major traversal, out-of-place
  transpose, naive and blocked matrix multiply, and a 2D stencil.

A `Hierarchy` is a list of heterogeneous levels -- `Cache`, `Scratchpad`
(explicitly addressed memory like GPU shared memory, no tags/replacement),
and a terminal `Backing` level -- so the same API can describe a CPU
L1/L2/L3/DRAM stack or a GPU-style global/L2/shared-memory/DRAM stack.

## Non-goals

This is a single-stream, address-pattern-level model, not a cycle-accurate
hardware simulator. It does not model: GPU bank conflicts, warp-level memory
coalescing, multi-core cache coherence, symbolic/closed-form miss-count
formulas, texture-cache-specific addressing, or automatic tile-size search
(that's a job for an autotuner working against real hardware, such as
[cutile-rs](https://github.com/NVlabs/cutile-rs)'s).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
