//! Unified event type for feed manager

use common::{L2Update, LOBUpdate, FeatureFrame};
use serde::{Deserialize, Serialize};

/// Market data event that can be published to event bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    /// L2 depth update
    L2Update(L2Update),
    /// LOB snapshot update
    LOBUpdate(LOBUpdate),
    /// Feature frame update
    FeatureUpdate(FeatureFrame),
}

impl bus::Message for MarketEvent {}