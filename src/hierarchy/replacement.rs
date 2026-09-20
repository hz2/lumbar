/// Which line to evict from a full cache set.
///
/// Marked `#[non_exhaustive]` so new policies can be added without a breaking
/// change; custom user-defined policies are not supported in this version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ReplacementPolicy {
    #[default]
    Lru,
    Fifo,
    Random,
}
