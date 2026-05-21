//! QuaQue-inspired versioning for the knowledge graph.
//!
//! Based on arXiv:2603.18654 — condensed relational model using bitstring validity.
//! Each entity/relation row carries a `validity` bitstring; bit position is determined
//! by a version's `bit_slot` (0–63), NOT by its `id`.  Slots are reclaimable: when a
//! version is deleted its slot is freed and the next `create_version` call reuses the
//! lowest available slot.  A row belongs to version V when
//! `(validity & (1 << V.bit_slot)) != 0`.  Version filtering uses bitwise operations.

pub mod diff;
pub mod merge;
pub mod query;
pub mod snapshot;
pub mod store;

use serde::{Deserialize, Serialize};

/// Metadata for a named version/snapshot of the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub id: i64,
    pub name: String,
    pub branch: String,
    pub parent_id: Option<i64>,
    pub description: Option<String>,
    pub created_at: Option<i64>,
    pub is_merged: bool,
}

/// Result of comparing two versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    pub added_entities: Vec<crate::graph::Entity>,
    pub removed_entities: Vec<crate::graph::Entity>,
    pub common_entities: Vec<crate::graph::Entity>,
    pub added_relations: Vec<crate::graph::Relation>,
    pub removed_relations: Vec<crate::graph::Relation>,
    pub common_relations: Vec<crate::graph::Relation>,
}

/// Strategy for merging versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Entity/relation exists in merged version if it exists in ANY source version.
    Union,
    /// Entity/relation exists in merged version only if it exists in ALL source versions.
    Intersection,
}

/// Maximum number of concurrent versions, bounded by the 64 usable bits of a
/// signed 64-bit `validity` column.
pub(crate) const MAX_VERSIONS: i64 = 64;

/// Compute the validity bitmask for a `bit_slot` in `[0, 63]`.
///
/// Slots — not version ids — drive the bit position so the limit is exactly 64
/// *concurrent* versions (slots are reclaimed on delete) instead of 64 version
/// *creations* over the database lifetime.  Callers obtain a version's slot via
/// [`store::version_bit_for`], which validates existence; this helper only does
/// the shift and assumes a validated slot.
#[inline]
pub(crate) fn bit_from_slot(slot: i64) -> i64 {
    debug_assert!((0..MAX_VERSIONS).contains(&slot), "bit_slot out of range");
    1 << slot
}
