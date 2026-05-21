## 1. Schema & Foundation

- [x] 1.1 Add migration v4 to `src/schema.rs`: create `kg_versions` table (id, name UNIQUE, branch, parent_id, description, created_at, is_merged) with indexes on branch and parent_id
- [x] 1.2 Add `validity INTEGER DEFAULT NULL` column to `kg_entities` and `kg_relations` in migration v4
- [x] 1.3 Bump `CURRENT_SCHEMA_VERSION` from 3 to 4
- [x] 1.4 Add schema v4 tests: fresh DB reaches v4, legacy DB migrates, new columns writable, kg_versions table exists

## 2. Version Store

- [x] 2.1 Create `src/version/mod.rs` with `Version` struct (id, name, branch, parent_id, description, created_at, is_merged) and public re-exports
- [x] 2.2 Create `src/version/store.rs` with `create_version`, `delete_version`, `list_versions` functions
- [x] 2.3 Add `version_create`, `version_delete`, `version_list` methods to `KnowledgeGraph` in `src/lib.rs`
- [x] 2.4 Register `version` module in `src/lib.rs` and add re-exports for `Version`
- [x] 2.5 Add tests for version CRUD: create, duplicate name rejection, delete, delete non-existent, list all, list by branch

## 3. Bitwise Function

- [x] 3.1 Add `kg_bit_count(x)` scalar function to `src/functions.rs` using `x.count_ones()`, handling NULL → 0
- [x] 3.2 Add tests for kg_bit_count: positive integer, zero, NULL

## 4. Version Snapshot

- [x] 4.1 Create `src/version/snapshot.rs` with `version_add_entity`, `version_remove_entity`, `version_add_relation`, `version_remove_relation`
- [x] 4.2 Implement NULL → bitstring handling via `COALESCE(validity, 0) | (1 << bit)` and zero-to-NULL on removal
- [x] 4.3 Implement `version_snapshot_entities` (bulk add all entities to a version in one transaction)
- [x] 4.4 Add corresponding `version_*` methods to `KnowledgeGraph`
- [x] 4.5 Add tests: add unversioned entity to version, add to additional version, remove from version, remove from only version (→ NULL), bulk snapshot, error on non-existent entity/version

## 5. Version Query

- [x] 5.1 Create `src/version/query.rs` with `version_entities` and `version_relations` — filtered by `(validity & (1 << bit)) != 0`, excluding NULL
- [x] 5.2 Implement `version_neighbors` — BFS traversal where both entity and connecting relation must exist in the version
- [x] 5.3 Add corresponding `version_*` methods to `KnowledgeGraph`
- [x] 5.4 Add tests: entities in version, entities with type filter, relations in version, relations with rel_type filter, neighbors filtered by version, neighbor entity excluded when not in version

## 6. Version Diff

- [x] 6.1 Create `src/version/diff.rs` with `VersionDiff` struct (added/removed/common entities and relations)
- [x] 6.2 Implement `version_compare` — compute added/removed/common using bitwise checks per entity and relation
- [x] 6.3 Implement `version_entity_history` — return all versions containing a given entity
- [x] 6.4 Add corresponding methods to `KnowledgeGraph` and re-export `VersionDiff`
- [x] 6.5 Add tests: added entities, removed entities, common entities, relations diff, entity history for multi-version entity, entity history for unversioned entity

## 7. Version Merge

- [x] 7.1 Create `src/version/merge.rs` with `version_merge` function accepting source_ids, target_name, and strategy (union|intersection)
- [x] 7.2 Implement union merge: create new version, set bit for entities/relations present in ANY source version (bitwise OR)
- [x] 7.3 Implement intersection merge: create new version, set bit only for entities/relations present in ALL source versions (bitwise AND)
- [x] 7.4 Add validation: reject < 2 source versions, reject non-existent version IDs
- [x] 7.5 Add corresponding method to `KnowledgeGraph`
- [x] 7.6 Add tests: union merge of two versions, intersection merge, merge creates new version row (parent_id, is_merged=1), invalid merge detection

## 8. Integration & Polish

- [x] 8.1 Run `cargo test` — all existing tests pass (no regressions)
- [x] 8.2 Run `cargo clippy -- -D warnings` — zero warnings
- [x] 8.3 Run `cargo fmt` — zero diff
- [x] 8.4 Verify `cargo test --features async` passes (if async feature is used)
