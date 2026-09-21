use std::collections::HashMap;

use crate::access::Access;
use crate::hierarchy::cache_config::CacheConfig;
use crate::simulator::LevelStats;

/// Simulates Belady's MIN algorithm (the provably optimal offline cache
/// replacement policy) against a single cache level, as a ceiling to compare
/// online policies like LRU against.
///
/// Unlike [`crate::Simulator`], this needs the whole trace up front: at every
/// eviction it decides based on which resident line is used furthest in the
/// future (or never again), which is knowledge no real, causal replacement
/// policy has access to. It models only one cache level's hit/miss behavior
/// -- no multi-level propagation, no write policy, no dirty tracking --
/// since it exists as a theoretical reference point, not a hardware model.
/// This is why it is a standalone function rather than a [`crate::ReplacementPolicy`]
/// variant: every variant `Simulator` supports must work from a single
/// access at a time, which Belady's algorithm cannot.
pub fn belady_optimal(cache: &CacheConfig, trace: &[Access]) -> LevelStats {
    let line_size = cache.line_size_bytes() as u64;
    let ways = cache.ways();
    let num_sets = cache.num_sets();

    let blocks: Vec<u64> = trace
        .iter()
        .map(|access| access.address / line_size)
        .collect();
    let next_use = next_occurrence_indices(&blocks);

    let mut tags: Vec<Vec<Option<u64>>> = vec![vec![None; ways]; num_sets];
    // next_ref[set][way]: trace index at which that way's current content is
    // next referenced, usize::MAX if never again.
    let mut next_ref: Vec<Vec<usize>> = vec![vec![usize::MAX; ways]; num_sets];

    let mut hits = 0u64;
    let mut misses = 0u64;

    for (i, &block) in blocks.iter().enumerate() {
        let set_index = (block % num_sets as u64) as usize;
        let set_tags = &mut tags[set_index];
        let set_next = &mut next_ref[set_index];

        if let Some(way) = set_tags.iter().position(|tag| *tag == Some(block)) {
            hits += 1;
            set_next[way] = next_use[i];
            continue;
        }

        misses += 1;
        let victim = set_tags
            .iter()
            .position(|tag| tag.is_none())
            .unwrap_or_else(|| {
                (0..ways)
                    .max_by_key(|&way| set_next[way])
                    .expect("ways > 0")
            });
        set_tags[victim] = Some(block);
        set_next[victim] = next_use[i];
    }

    LevelStats {
        name: "belady".to_string(),
        hits,
        misses,
    }
}

/// For each position, the index of the next occurrence of that same block
/// later in the slice, or `usize::MAX` if it never recurs.
fn next_occurrence_indices(blocks: &[u64]) -> Vec<usize> {
    let mut next = vec![usize::MAX; blocks.len()];
    let mut last_seen: HashMap<u64, usize> = HashMap::new();
    for i in (0..blocks.len()).rev() {
        if let Some(&seen_at) = last_seen.get(&blocks[i]) {
            next[i] = seen_at;
        }
        last_seen.insert(blocks[i], i);
    }
    next
}
