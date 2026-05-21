## Why

The knowledge graph has no versioning — once an entity or relation is created, there is no way to snapshot the graph state, compare snapshots, or branch/merge. This makes it impossible to track how the KG evolves over time, compare two research snapshots, or roll back to a known-good state. The QuaQue paper (arXiv:2603.18654) describes a condensed relational model where a single bitstring column tracks which versions a row belongs to, enabling fast bitwise version filtering without duplicating data. This maps directly to our SQLite property graph.

## What Changes

- Add a `kg_versions` table to track version metadata (name, branch, parent, timestamps, and a reclaimable `bit_slot`)
- Add a `validity INTEGER DEFAULT NULL` column to `kg_entities` and `kg_relations` — a 64-bit bitstring where the bit at a version's `bit_slot` is set when the row exists in that version
- Add a new `src/version/` domain module with version CRUD, snapshot, query, diff, and merge operations
- Register a `kg_bit_count()` SQLite custom function for version-aware aggregation queries
- Add ~15 new methods to `KnowledgeGraph` (prefixed `version_`) — existing API is unchanged
- Migration v4 in the existing schema versioning system

## Capabilities

### New Capabilities

- `version-store`: Version lifecycle — create, delete, list versions with branch/parent tracking. The `kg_versions` table and CRUD operations.
- `version-snapshot`: Assigning entities/relations to versions via bitstring manipulation (add/remove from version, bulk snapshot, COALESCE-based NULL handling for backward compat).
- `version-query`: Version-filtered queries — get entities/relations for a specific version, version-aware neighbor traversal, version-aware graph stats. Uses `(validity & (1 << bit)) != 0` filtering.
- `version-diff`: Comparing two versions — added/removed entities and relations. Bitwise intersection/exclusion to compute diffs efficiently.
- `version-merge`: Merging versions with union or intersection strategy. Branching via parent_id. Bitwise OR/AND operations on validity columns.

### Modified Capabilities

(None — all version functionality is additive. Existing API signatures are unchanged.)

## Impact

- **Schema**: Migration v4 adds one table (`kg_versions`, including a `bit_slot INTEGER NOT NULL UNIQUE`) and two columns (`validity` on `kg_entities`, `kg_relations`). Non-destructive — existing rows get `validity=NULL`.
- **New module**: `src/version/` with ~5 files (mod.rs, store.rs, snapshot.rs, query.rs, diff.rs, merge.rs)
- **Functions**: `src/functions.rs` gains `kg_bit_count()` registration (rusqlite path)
- **Public API**: `KnowledgeGraph` gains ~15 new `version_*` methods. `lib.rs` re-exports new types. New error variant `VersionLimitExceeded`.
- **Schema version**: `CURRENT_SCHEMA_VERSION` bumps from 3 to 4
- **Dependencies**: No new crate dependencies — uses existing `rusqlite` for bitwise SQL operations
- **SQLite extension**: The `cdylib` build registers `kg_bit_count` explicitly in `extension::register_extension_functions` (the loadable-extension path; the rusqlite `register_functions` registration does not apply to `.load`-ed connections)
