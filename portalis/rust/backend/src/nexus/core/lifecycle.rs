//! Durable collection intent.
//!
//! The lifecycle is defined in `store::records` because it is persisted, not
//! reconstructed. Re-exporting it here keeps workers phrased in domain terms
//! without creating a second model that could drift from the row on disk.

pub use crate::nexus::store::records::{StoredActivity as Activity, StoredLifecycle as Lifecycle};
