use std::collections::BTreeMap;

use crate::access::Address;

/// LRU stack distances computed from an address stream, bucketed by distance.
///
/// Built by [`reuse_distances`]. Answers "what hit rate would an idealized
/// fully-associative LRU cache of a given size get on this stream" for every
/// size at once, without re-simulating per size.
#[derive(Clone, Debug, Default)]
pub struct ReuseDistanceHistogram {
    distance_counts: BTreeMap<usize, u64>,
    cold_count: u64,
    total: u64,
}

impl ReuseDistanceHistogram {
    /// Idealized fully-associative-LRU hit rate for a cache holding
    /// `capacity_lines` lines. Idealized: no conflict misses, no associativity
    /// limits, no coexisting traffic -- a real set-associative cache of the
    /// same size will generally do worse. Use [`crate::Simulator`] for a
    /// concrete hierarchy's actual hit rate.
    pub fn hit_rate_at_capacity(&self, capacity_lines: usize) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        let hits: u64 = self
            .distance_counts
            .range(..capacity_lines)
            .map(|(_, count)| count)
            .sum();
        hits as f64 / self.total as f64
    }

    /// Smallest capacity, in lines, at which the idealized hit rate reaches
    /// `target_hit_rate`. `None` if no capacity (including unbounded) reaches
    /// it, e.g. because too many references are cold (first-touch) misses.
    pub fn capacity_for_hit_rate(&self, target_hit_rate: f64) -> Option<usize> {
        if self.total == 0 {
            return None;
        }
        let mut cumulative = 0u64;
        for (&distance, &count) in &self.distance_counts {
            cumulative += count;
            if cumulative as f64 / self.total as f64 >= target_hit_rate {
                return Some(distance + 1);
            }
        }
        None
    }

    /// Total accesses the histogram was built from, including cold misses.
    pub fn total_accesses(&self) -> u64 {
        self.total
    }

    /// Accesses that were a first reference to their address (infinite stack
    /// distance -- always a miss at any finite capacity).
    pub fn cold_accesses(&self) -> u64 {
        self.cold_count
    }
}

/// Computes LRU stack distances for an address stream (the Mattson et al.
/// stack algorithm): for each access, the number of distinct addresses
/// referenced since the address's own last reference, or "cold" for a first
/// reference.
///
/// `addresses` are opaque identifiers -- callers choose the granularity (e.g.
/// pass block-aligned addresses, `raw_address / line_size`, to measure
/// line-based reuse distance rather than byte-based).
///
/// This is a straightforward O(N x D) implementation, where D is the number
/// of distinct addresses seen so far, not the asymptotically optimal O(N log
/// N) one -- simplicity over cleverness for a first version.
///
/// ```
/// use lumbar::reuse_distances;
///
/// // 0 is re-referenced after seeing exactly one other distinct address (1),
/// // so its stack distance is 1 -- it hits at any capacity >= 2 lines.
/// let histogram = reuse_distances([0u64, 1, 0]);
/// assert_eq!(histogram.hit_rate_at_capacity(2), 1.0 / 3.0);
/// assert_eq!(histogram.cold_accesses(), 2); // the first 0 and the first 1
/// ```
pub fn reuse_distances<I: IntoIterator<Item = Address>>(addresses: I) -> ReuseDistanceHistogram {
    let mut stack: Vec<Address> = Vec::new();
    let mut histogram = ReuseDistanceHistogram::default();

    for address in addresses {
        histogram.total += 1;
        match stack.iter().position(|&a| a == address) {
            Some(pos) => {
                *histogram.distance_counts.entry(pos).or_insert(0) += 1;
                stack.remove(pos);
            }
            None => {
                histogram.cold_count += 1;
            }
        }
        stack.insert(0, address);
    }

    histogram
}
