## Context

The sqlite-knowledge-graph library (schema v3) stores entities, relations, vectors, and hyperedges in SQLite. There is no versioning — the graph is a single mutable state. The QuaQue paper (arXiv:2603.18654) describes a condensed relational model where each row carries a bitstring column (`validity`) whose Nth bit indicates the row belongs to version N. Version filtering becomes bitwise AND, version merging becomes bitwise OR/AND — all O(1) per row with no data duplication.

Our property graph differs from RDF quads (entities have mutable properties, relations have weights), but the bitstring existence-tracking maps cleanly to snapshot semantics: a version captures *which* entities and relations exist, not their property values at that point in time.

Current schema: `kg_entities`, `kg_relations`, `kg_vectors`, `kg_hyperedges`, `kg_hyperedge_entities`, `kg_turboquant_cache`, `kg_schema_version`, `kg_dependencies`, `kg_confidence_log`. Migration system is at v3 with `ensure_schema()` auto-migration.

## Goals / Non-Goals

**Goals:**
- Snapshot the KG at named version points
- Query the graph as it existed at a specific version
- Compare two versions (added/removed entities and relations)
- Merge versions with union or intersection strategy
- Support branching via parent_id lineage
- Full backward compatibility — existing code and databases work unchanged

**Non-Goals:**
- Property-level change tracking across versions (bitstring tracks existence only)
- Version-aware vectors, hyperedges, or algorithms (PageRank, Louvain — these operate on the full graph)
- More than 64 concurrent versions (SQLite INTEGER is 64-bit signed)
- Conflict resolution beyond union/intersection
- Auto-versioning on every write

## Decisions

### 1. Opt-in domain module vs versioned core

**Decision**: New `src/version/` module with `version_*` methods on `KnowledgeGraph`. Existing API untouched.

**Rationale**: The library has ~30 public methods. Adding an optional `version_id` parameter to every one would be a breaking change and bloat signatures for callers who don't need versioning. A separate domain module follows the existing pattern (algorithms, vector, rag are all domain modules).

**Alternative considered**: Version parameter on every function. Rejected — too invasive, and algorithms like PageRank don't have a meaningful per-version scope.

### 2. NULL validity = unversioned, visible everywhere

**Decision**: `validity INTEGER DEFAULT NULL`. NULL means "not participating in versioning, visible in all queries". Non-NULL bitstring means "exists only in these versions".

**Rationale**: Backward compatibility. Existing rows get NULL after migration v4. Existing queries (no version filter) return everything. Only version-aware queries use the bitstring.

**Key SQL pattern**: `COALESCE(validity, 0) | (1 << bit)` handles first-time assignment from NULL.

### 3. SQLite INTEGER for bitstring (64 *concurrent* versions)

**Decision**: Use native SQLite INTEGER (signed 64-bit). No BLOB fallback. The limit is 64 *concurrent* live versions; slots are reclaimed on delete (see Decision 4).

**Rationale**: 64 concurrent versions is generous for snapshot use cases. INTEGER supports native bitwise operators (`&`, `|`, `~`, `<<`). No custom BLOB functions needed. If the limit is ever hit, `create_version` returns `Error::VersionLimitExceeded` (a clean, recoverable error — never a panic or silent corruption). Raising the ceiling would require a schema redesign (new migration to BLOB + custom functions).

### 4. Version bit position = a reclaimable `bit_slot`, not the auto-increment id

**Decision**: Each `kg_versions` row carries a `bit_slot` column in `[0, 63]` (`UNIQUE`), assigned as the lowest free slot at creation and freed on deletion. The validity bit is `1 << bit_slot`.

**Rationale**: An earlier draft used `1 << (version_id - 1)`. Because `version_id` comes from `AUTOINCREMENT` and is never reused, that made the limit "64 version *creations* over the database lifetime" and, worse, the 65th creation computed `1 << 64` — a panic in debug builds and a silent bit-aliasing corruption in release. Decoupling the bit position into a reclaimable slot makes the limit exactly 64 *concurrent* versions and removes the overflow entirely (`bit_from_slot` always shifts by 0–63). Deleting a version clears its bit from every entity/relation row in the same transaction, so a recycled slot can never inherit stale membership.

### 5. bit_count via custom scalar function (registered on both surfaces)

**Decision**: Register `kg_bit_count(x)` using `x.count_ones()`, NULL → 0.

**Rationale**: SQLite has no built-in `bit_count()`. Needed for version-aware aggregation (e.g., "how many versions does this entity span"). It is registered in **two** places, because the library and the loadable extension use different registration paths: `functions::register_functions` (rusqlite path, used by `KnowledgeGraph::open*`) and `extension::register_extension_functions` (sqlite-loadable path, used by the `cdylib` `.load` entry point). Both must register it for raw-SQL availability on both surfaces.

### 6. Merge creates a new version row (atomically)

**Decision**: `version_merge(source_ids, target_name, strategy)` creates a new `kg_versions` row and updates validity columns on entities/relations using bitwise OR. Union sets the new bit where `(validity & source_mask) != 0`; intersection where `(validity & source_mask) = source_mask`. The whole operation — new version, `is_merged` flag, and both UPDATEs — runs in a single transaction.

**Rationale**: Non-destructive — source versions remain unchanged. The new version gets its own slot. Wrapping in a transaction means a mid-merge failure leaves no half-built version. The intersection predicate is a single set-based UPDATE rather than a per-row loop.

## Risks / Trade-offs

- **[64-version limit]** → Document clearly. For a snapshot tool this is generous. If hit, requires BLOB migration — a separate future change.
- **[NULL vs 0 ambiguity]** → Document convention: NULL = unversioned, 0 = "in versioning but in no version" (shouldn't occur). `version_remove_entity` should check if result would be 0 and either leave it or set NULL.
- **[Existing queries ignore versions]** → This is by design (backward compat). But it means a user querying raw SQL won't see version filtering unless they add WHERE clauses. Document the pattern.
- **[No property history]** → Explicitly out of scope. If users need it later, a `kg_entity_property_history` table can be added without changing the bitstring design.
- **[Migration v4 is non-destructive]** → ALTER TABLE ADD COLUMN is safe. No data loss possible. `validity=NULL` for all existing rows.
