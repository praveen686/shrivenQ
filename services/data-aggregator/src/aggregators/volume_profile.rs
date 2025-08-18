//! Volume profile aggregator

pub struct VolumeProfileAggregator {
    // Implementation details
}

impl Default for VolumeProfileAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl VolumeProfileAggregator {
    #[must_use] pub const fn new() -> Self {
        Self {}
    }
}
