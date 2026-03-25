# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Placeholder for upcoming features

---

## [0.3.0] - 2026-03-25

### Added

- **RAG Integration Module** (`src/rag/`)
  - `kg_semantic_search()` - Semantic search with vector similarity
  - `kg_get_context()` - Get entity context with related entities
  - `kg_hybrid_search()` - Combine keyword and semantic search

- **CLI Tool** (`sqlite-kg`)
  - `migrate` command - Migrate data from knowledge.db
  - `search` command - Search entities
  - `stats` command - Show knowledge graph statistics

- **Data Migration**
  - Migrated 2,497 papers from knowledge.db
  - Migrated 122 skills from knowledge.db
  - Built 1,480,951 relations between entities

### Changed

- Enhanced entity storage with metadata support
- Improved relation storage with confidence scores

### Technical

- 27 tests passing (22 unit + 5 integration)
- Vector storage using placeholder zero vectors

---

## [0.2.0] - 2026-03-24

### Added

- **Entity Storage Module** (`src/graph/entity.rs`)
  - `kg_create_entity()` - Create entities
  - `kg_get_entity()` - Get entity by ID
  - `kg_update_entity()` - Update entity properties
  - `kg_delete_entity()` - Delete entity
  - `kg_list_entities()` - List entities with filters

- **Relation Storage Module** (`src/graph/relation.rs`)
  - `kg_create_relation()` - Create relations
  - `kg_get_relation()` - Get relation by ID
  - `kg_delete_relation()` - Delete relation
  - `kg_get_related_entities()` - Get related entities

- **Vector Storage Module** (`src/vector/store.rs`)
  - `kg_set_embedding()` - Set entity embedding
  - `kg_get_embedding()` - Get entity embedding
  - `kg_find_similar()` - Find similar entities

- **Database Schema** (`src/schema.rs`)
  - Entities table with metadata JSON
  - Relations table with confidence scores
  - Embeddings table for vector storage

- **SQLite Custom Functions** (`src/functions.rs`)
  - All functions registered as SQLite UDFs

### Changed

- Fixed compilation errors (rusqlite features, module declarations)
- Added proper error handling with `thiserror`

### Technical

- 24 tests passing (19 unit + 5 integration)
- Production-ready core modules

---

## [0.1.0] - 2026-03-24

### Added

- Project initialization
- Rust project scaffolding with Cargo.toml
- Module structure: `graph/`, `vector/`, `rag/`
- Basic SQLite function registration
- MIT License
- README.md with project overview
- DEVLOG.md for development tracking
- Technical research report (`research.md`)

### Technical

- 1 test passing
- Project compiles successfully

---

## Version History

| Version | Date | Key Features |
|---------|------|--------------|
| 0.3.0 | 2026-03-25 | RAG integration, data migration |
| 0.2.0 | 2026-03-24 | Core modules (entity, relation, vector) |
| 0.1.0 | 2026-03-24 | Project initialization |

---

[Unreleased]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.3.0
[0.2.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.2.0
[0.1.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.1.0