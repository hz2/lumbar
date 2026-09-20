pub mod cache_config;
pub mod replacement;

use cache_config::CacheConfig;

/// What kind of level sits at one position in a [`Hierarchy`].
#[derive(Clone, Debug)]
pub enum LevelKind {
    /// A tagged, indexed cache with a replacement policy.
    Cache(CacheConfig),
    /// Explicitly addressed scratchpad memory (e.g. GPU shared memory): no
    /// tags or replacement, just a capacity limit. Not part of the hit/miss
    /// chain a [`crate::Simulator`] walks for a generic global-address trace
    /// -- accesses pass through it untouched, since a real scratchpad is a
    /// distinct address space, not something the same address stream misses
    /// into. It is still useful data for [`crate::advise_working_set`].
    Scratchpad {
        size_bytes: usize,
        latency_cycles: u32,
    },
    /// The terminal level (e.g. DRAM): always a hit, contributes only latency.
    Backing { latency_cycles: u32 },
}

/// One named level of a [`Hierarchy`].
#[derive(Clone, Debug)]
pub struct Level {
    pub name: String,
    pub kind: LevelKind,
}

/// An ordered stack of levels, fastest/closest first, ending in a
/// [`LevelKind::Backing`] level.
///
/// Levels are heterogeneous ([`LevelKind::Cache`], [`LevelKind::Scratchpad`],
/// [`LevelKind::Backing`]) so the same type can describe a CPU L1/L2/L3/DRAM
/// stack or a GPU-style global/L2/shared-memory/DRAM stack.
#[derive(Clone, Debug)]
pub struct Hierarchy {
    levels: Vec<Level>,
}

impl Hierarchy {
    pub fn builder() -> HierarchyBuilder {
        HierarchyBuilder { levels: Vec::new() }
    }

    pub fn levels(&self) -> &[Level] {
        &self.levels
    }
}

/// Builds a [`Hierarchy`] one level at a time, fastest first.
///
/// [`HierarchyBuilder::backing`] is the only way to obtain a [`Hierarchy`],
/// so every hierarchy is guaranteed to end in a terminal level.
pub struct HierarchyBuilder {
    levels: Vec<Level>,
}

impl HierarchyBuilder {
    #[must_use]
    pub fn add_cache(mut self, name: impl Into<String>, config: CacheConfig) -> Self {
        self.levels.push(Level {
            name: name.into(),
            kind: LevelKind::Cache(config),
        });
        self
    }

    #[must_use]
    pub fn add_scratchpad(
        mut self,
        name: impl Into<String>,
        size_bytes: usize,
        latency_cycles: u32,
    ) -> Self {
        self.levels.push(Level {
            name: name.into(),
            kind: LevelKind::Scratchpad {
                size_bytes,
                latency_cycles,
            },
        });
        self
    }

    pub fn backing(mut self, name: impl Into<String>, latency_cycles: u32) -> Hierarchy {
        self.levels.push(Level {
            name: name.into(),
            kind: LevelKind::Backing { latency_cycles },
        });
        Hierarchy {
            levels: self.levels,
        }
    }
}
