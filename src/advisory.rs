use std::collections::HashSet;

use crate::access::Address;
use crate::hierarchy::{Hierarchy, LevelKind};

/// An advisory (not a guarantee) about whether a working set fits within each
/// level of a [`Hierarchy`].
#[derive(Clone, Debug)]
pub struct WorkingSetAdvisory {
    pub distinct_bytes_touched: usize,
    pub fits_in: Vec<String>,
    pub does_not_fit_in: Vec<String>,
}

/// Estimates whether the distinct addresses in `addresses` fit within each
/// level of `hierarchy`, by size alone.
///
/// `unit_size_bytes` is how many bytes each distinct address represents (the
/// element size for raw byte addresses, or a level's line size if
/// `addresses` are already block-aligned).
///
/// This is advisory only: it is a capacity check, not a simulation. It does
/// not account for associativity, conflict misses, or other traffic sharing
/// the cache, so a working set marked "fits" can still miss heavily on real
/// set-associative hardware. Backing levels are omitted since they have no
/// meaningful capacity limit here.
///
/// ```
/// use lumbar::{Associativity, CacheConfigBuilder, Hierarchy, advise_working_set};
///
/// let l1 = CacheConfigBuilder::new(1024, 64, Associativity::DirectMapped).build().unwrap();
/// let hierarchy = Hierarchy::builder().add_cache("L1", l1).backing("mem", 0);
///
/// // 512 bytes of distinct addresses, 4 bytes per address: fits in the 1KiB L1
/// let advisory = advise_working_set(&hierarchy, 0..128, 4);
/// assert_eq!(advisory.distinct_bytes_touched, 512);
/// assert_eq!(advisory.fits_in, vec!["L1"]);
/// ```
pub fn advise_working_set(
    hierarchy: &Hierarchy,
    addresses: impl IntoIterator<Item = Address>,
    unit_size_bytes: usize,
) -> WorkingSetAdvisory {
    let distinct: HashSet<Address> = addresses.into_iter().collect();
    let distinct_bytes_touched = distinct.len().saturating_mul(unit_size_bytes);

    let mut fits_in = Vec::new();
    let mut does_not_fit_in = Vec::new();
    for level in hierarchy.levels() {
        let capacity = match &level.kind {
            LevelKind::Cache(cfg) => Some(cfg.size_bytes()),
            LevelKind::Scratchpad { size_bytes, .. } => Some(*size_bytes),
            LevelKind::Backing { .. } => None,
        };
        let Some(capacity) = capacity else { continue };
        if distinct_bytes_touched <= capacity {
            fits_in.push(level.name.clone());
        } else {
            does_not_fit_in.push(level.name.clone());
        }
    }

    WorkingSetAdvisory {
        distinct_bytes_touched,
        fits_in,
        does_not_fit_in,
    }
}
