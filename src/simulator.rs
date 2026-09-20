use std::collections::VecDeque;

use crate::access::{Access, AccessKind, Address};
use crate::hierarchy::cache_config::{CacheConfig, WritePolicy};
use crate::hierarchy::replacement::ReplacementPolicy;
use crate::hierarchy::{Hierarchy, LevelKind};

/// Hit/miss counts for one level after a simulation run.
#[derive(Clone, Debug)]
pub struct LevelStats {
    pub name: String,
    pub hits: u64,
    pub misses: u64,
}

impl LevelStats {
    pub fn accesses(&self) -> u64 {
        self.hits + self.misses
    }

    /// Fraction of accesses to this level that hit. `0.0` if the level was
    /// never accessed.
    pub fn hit_rate(&self) -> f64 {
        let total = self.accesses();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// The outcome of running a trace through a [`Simulator`].
#[derive(Clone, Debug)]
pub struct SimulationResult {
    /// One entry per non-scratchpad level, in hierarchy order.
    pub per_level: Vec<LevelStats>,
    /// Average memory access time, in the latency units the [`Hierarchy`]'s
    /// levels were configured with.
    pub amat_cycles: f64,
}

struct CacheState {
    config: CacheConfig,
    ways: usize,
    num_sets: usize,
    sets: Vec<CacheSet>,
    hits: u64,
    misses: u64,
    writebacks: u64,
}

impl CacheState {
    fn new(config: CacheConfig) -> Self {
        let ways = config.ways();
        let num_sets = config.num_sets();
        Self {
            config,
            ways,
            num_sets,
            sets: (0..num_sets).map(|_| CacheSet::new(ways)).collect(),
            hits: 0,
            misses: 0,
            writebacks: 0,
        }
    }
}

struct CacheSet {
    tags: Vec<Option<u64>>,
    dirty: Vec<bool>,
    /// Insertion order of currently-valid ways; front = oldest. Only
    /// maintained/consulted for [`ReplacementPolicy::Fifo`].
    fifo: VecDeque<usize>,
    /// Recency order of currently-valid ways; front = most recently used.
    /// Only maintained/consulted for [`ReplacementPolicy::Lru`].
    lru: Vec<usize>,
}

impl CacheSet {
    fn new(ways: usize) -> Self {
        Self {
            tags: vec![None; ways],
            dirty: vec![false; ways],
            fifo: VecDeque::with_capacity(ways),
            lru: Vec::with_capacity(ways),
        }
    }
}

struct BackingState {
    reached: u64,
}

enum LevelState {
    Cache(CacheState),
    Scratchpad,
    Backing(BackingState),
}

/// Walks a stream of [`Access`]es through a configured [`Hierarchy`],
/// producing per-level hit/miss statistics and an AMAT estimate.
///
/// A single [`Access`] may span more than one cache line; it is split at the
/// granularity of the hierarchy's fastest cache level (or, if there is none,
/// at its own size) so line-crossing accesses are counted correctly.
pub struct Simulator {
    hierarchy: Hierarchy,
    levels: Vec<LevelState>,
    rng_state: u64,
}

impl Simulator {
    pub fn new(hierarchy: Hierarchy) -> Self {
        let levels = hierarchy
            .levels()
            .iter()
            .map(|level| match &level.kind {
                LevelKind::Cache(cfg) => LevelState::Cache(CacheState::new(*cfg)),
                LevelKind::Scratchpad { .. } => LevelState::Scratchpad,
                LevelKind::Backing { .. } => LevelState::Backing(BackingState { reached: 0 }),
            })
            .collect();
        // fixed, arbitrary non-zero seed: deterministic runs by default
        Self {
            hierarchy,
            levels,
            rng_state: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// Runs an entire trace and returns the accumulated [`SimulationResult`].
    pub fn run<I: IntoIterator<Item = Access>>(&mut self, trace: I) -> SimulationResult {
        for access in trace {
            self.step(access);
        }
        self.result()
    }

    /// Feeds a single access through the hierarchy, for streaming use over
    /// traces too large to collect.
    pub fn step(&mut self, access: Access) {
        let granularity = self
            .first_cache_line_size()
            .unwrap_or_else(|| access.size.max(1) as u64);
        let start_block = access.address / granularity;
        let end_addr = access.address + access.size.max(1) as u64 - 1;
        let end_block = end_addr / granularity;
        for block in start_block..=end_block {
            self.access_level(0, block * granularity, access.kind);
        }
    }

    /// Snapshots the current hit/miss counts and AMAT without consuming the
    /// simulator, so `step` can be interleaved with reporting.
    pub fn result(&self) -> SimulationResult {
        let per_level = self
            .hierarchy
            .levels()
            .iter()
            .zip(&self.levels)
            .filter_map(|(level, state)| match state {
                LevelState::Cache(cache) => Some(LevelStats {
                    name: level.name.clone(),
                    hits: cache.hits,
                    misses: cache.misses,
                }),
                LevelState::Backing(backing) => Some(LevelStats {
                    name: level.name.clone(),
                    hits: backing.reached,
                    misses: 0,
                }),
                LevelState::Scratchpad => None,
            })
            .collect();
        SimulationResult {
            per_level,
            amat_cycles: self.compute_amat(),
        }
    }

    fn first_cache_line_size(&self) -> Option<u64> {
        self.hierarchy
            .levels()
            .iter()
            .find_map(|level| match &level.kind {
                LevelKind::Cache(cfg) => Some(cfg.line_size_bytes() as u64),
                _ => None,
            })
    }

    fn compute_amat(&self) -> f64 {
        let mut amat = 0.0;
        for (level, state) in self.hierarchy.levels().iter().zip(&self.levels).rev() {
            match (state, &level.kind) {
                (LevelState::Backing(_), LevelKind::Backing { latency_cycles }) => {
                    amat = *latency_cycles as f64;
                }
                (LevelState::Cache(cache), LevelKind::Cache(cfg)) => {
                    let total = cache.hits + cache.misses;
                    let miss_rate = if total == 0 {
                        0.0
                    } else {
                        cache.misses as f64 / total as f64
                    };
                    amat = cfg.latency_cycles() as f64 + miss_rate * amat;
                }
                (LevelState::Scratchpad, LevelKind::Scratchpad { .. }) => {
                    // excluded from the global-address AMAT chain, see LevelKind::Scratchpad
                }
                _ => unreachable!("level state must match its level kind"),
            }
        }
        amat
    }

    fn access_level(&mut self, level_idx: usize, address: Address, kind: AccessKind) {
        match &self.levels[level_idx] {
            LevelState::Cache(_) => self.access_cache(level_idx, address, kind),
            LevelState::Scratchpad => {
                if level_idx + 1 < self.levels.len() {
                    self.access_level(level_idx + 1, address, kind);
                }
            }
            LevelState::Backing(_) => {
                if let LevelState::Backing(state) = &mut self.levels[level_idx] {
                    state.reached += 1;
                }
            }
        }
    }

    fn access_cache(&mut self, level_idx: usize, address: Address, kind: AccessKind) {
        let (line_size, ways, num_sets, write_policy, write_allocate, policy) = {
            let LevelState::Cache(state) = &self.levels[level_idx] else {
                unreachable!("access_cache called on a non-cache level")
            };
            (
                state.config.line_size_bytes() as u64,
                state.ways,
                state.num_sets,
                state.config.write_policy(),
                state.config.write_allocate(),
                state.config.replacement_policy(),
            )
        };
        let block = address / line_size;
        let set_index = (block % num_sets as u64) as usize;
        let has_next = level_idx + 1 < self.levels.len();

        let hit_way = {
            let LevelState::Cache(state) = &self.levels[level_idx] else {
                unreachable!()
            };
            state.sets[set_index]
                .tags
                .iter()
                .position(|tag| *tag == Some(block))
        };

        if let Some(way) = hit_way {
            if let LevelState::Cache(state) = &mut self.levels[level_idx] {
                state.hits += 1;
            }
            self.touch_recency(level_idx, set_index, way, policy);
            if kind == AccessKind::Write {
                if let LevelState::Cache(state) = &mut self.levels[level_idx] {
                    state.sets[set_index].dirty[way] = write_policy == WritePolicy::WriteBack;
                }
                if write_policy == WritePolicy::WriteThrough && has_next {
                    self.access_level(level_idx + 1, address, AccessKind::Write);
                }
            }
            return;
        }

        if let LevelState::Cache(state) = &mut self.levels[level_idx] {
            state.misses += 1;
        }

        let allocate = kind == AccessKind::Read || write_allocate;
        if !allocate {
            // no-write-allocate: the write bypasses this level entirely
            if has_next {
                self.access_level(level_idx + 1, address, AccessKind::Write);
            }
            return;
        }

        // fetch the line to install it here
        if has_next {
            self.access_level(level_idx + 1, address, AccessKind::Read);
        }
        // write-through also propagates the write itself immediately
        if kind == AccessKind::Write && write_policy == WritePolicy::WriteThrough && has_next {
            self.access_level(level_idx + 1, address, AccessKind::Write);
        }

        let victim_way = self.select_victim(level_idx, set_index, ways, policy);
        let evicted_dirty = {
            let LevelState::Cache(state) = &mut self.levels[level_idx] else {
                unreachable!()
            };
            let set = &mut state.sets[set_index];
            let was_dirty = set.tags[victim_way].is_some() && set.dirty[victim_way];
            set.tags[victim_way] = Some(block);
            set.dirty[victim_way] =
                kind == AccessKind::Write && write_policy == WritePolicy::WriteBack;
            was_dirty
        };
        if evicted_dirty && let LevelState::Cache(state) = &mut self.levels[level_idx] {
            state.writebacks += 1;
        }
        self.update_recency_after_install(level_idx, set_index, victim_way, policy);
    }

    fn select_victim(
        &mut self,
        level_idx: usize,
        set_index: usize,
        ways: usize,
        policy: ReplacementPolicy,
    ) -> usize {
        let rand_choice = if policy == ReplacementPolicy::Random {
            Some(self.next_rand() as usize % ways)
        } else {
            None
        };
        let LevelState::Cache(state) = &mut self.levels[level_idx] else {
            unreachable!()
        };
        let set = &mut state.sets[set_index];
        if let Some(empty) = set.tags.iter().position(|tag| tag.is_none()) {
            return empty;
        }
        match policy {
            ReplacementPolicy::Lru => *set.lru.last().expect("full set has ways > 0 entries"),
            ReplacementPolicy::Fifo => *set.fifo.front().expect("full set has ways > 0 entries"),
            ReplacementPolicy::Random => rand_choice.expect("computed above for Random policy"),
        }
    }

    fn touch_recency(
        &mut self,
        level_idx: usize,
        set_index: usize,
        way: usize,
        policy: ReplacementPolicy,
    ) {
        if policy != ReplacementPolicy::Lru {
            return;
        }
        let LevelState::Cache(state) = &mut self.levels[level_idx] else {
            unreachable!()
        };
        let lru = &mut state.sets[set_index].lru;
        if let Some(pos) = lru.iter().position(|&w| w == way) {
            lru.remove(pos);
        }
        lru.insert(0, way);
    }

    fn update_recency_after_install(
        &mut self,
        level_idx: usize,
        set_index: usize,
        way: usize,
        policy: ReplacementPolicy,
    ) {
        let LevelState::Cache(state) = &mut self.levels[level_idx] else {
            unreachable!()
        };
        let set = &mut state.sets[set_index];
        match policy {
            ReplacementPolicy::Lru => {
                if let Some(pos) = set.lru.iter().position(|&w| w == way) {
                    set.lru.remove(pos);
                }
                set.lru.insert(0, way);
            }
            ReplacementPolicy::Fifo => {
                if let Some(pos) = set.fifo.iter().position(|&w| w == way) {
                    set.fifo.remove(pos);
                }
                set.fifo.push_back(way);
            }
            ReplacementPolicy::Random => {}
        }
    }

    // xorshift64: enough for eviction tie-breaking, not for anything security-sensitive
    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }
}
