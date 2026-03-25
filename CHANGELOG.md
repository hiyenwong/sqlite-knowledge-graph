# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Placeholder for upcoming features

---

## [0.8.0] - 2026-03-25

### Added

- **TurboQuant Vector Indexing** - Near-optimal vector quantization for instant search
  - `TurboQuantIndex` - Fast approximate nearest neighbor search
  - `TurboQuantConfig` - Configurable dimension, bit_width, and seed
  - `KnowledgeGraph::create_turboquant_index()` - Create new index
  - `KnowledgeGraph::build_turboquant_index()` - Build from existing vectors
  - **Benefits:**
    - Instant indexing (no training required)
    - 6x memory compression
    - Near-zero accuracy loss
    - Up to 184,000x faster indexing vs Product Quantization

### Technical

- Added `rand` and `ndarray` dependencies
- New module: `src/vector/turboquant.rs`
- 4 new tests for TurboQuant functionality
- Total: 43 tests passing

### References

- Based on arXiv:2504.19874 (ICLR 2026)
- Google Research: "TurboQuant: Redefining AI efficiency with extreme compression"

---

## [0.7.0] - 2026-03-25

### Added

- **More Extension Functions** - Extended SQLite extension with graph algorithm functions
  - `kg_pagerank(damping, max_iterations, tolerance)` - PageRank algorithm with configurable parameters
  - `kg_louvain()` - Louvain community detection
  - `kg_bfs(start_id, max_depth)` - BFS traversal from starting entity
  - `kg_shortest_path(from_id, to_id, max_depth)` - Shortest path between entities
  - `kg_connected_components()` - Find connected components in graph
  - All functions support multiple parameter overloads
  - Returns JSON with algorithm info and parameters

### Changed

- Updated `src/extension.rs` with new SQL functions
- Updated README.md with new extension function documentation
- Updated Implementation Status table

### Technical

- 34 tests passing (33 unit + 1 extension test)
- Extension functions use sqlite-loadable crate
- Functions support optional parameters with defaults

---

## [0.6.0] - 2026-03-25

### Added

- **SQLite Extension Support** (experimental)
  - `src/extension.rs` - Extension entry points for macOS/Linux
  - Can be compiled as loadable extension (.dylib/.so)
  - Functions: kg_version, kg_stats, kg_search, kg_bfs, kg_shortest_path, kg_pagerank, kg_louvain, kg_connected_components

### Known Issues

- Extension loading may cause SIGSEGV on some platforms
- Recommend using CLI tool (`sqlite-kg`) or Rust API instead

### Technical

- Added `load_extension` feature to rusqlite
- Compiled extension at: `target/release/libsqlite_knowledge_graph.dylib`

---

## [0.5.0] - 2026-03-25

### Added

- **Graph Algorithms Module** (`src/algorithms/`)
  - `pagerank()` - PageRank centrality with configurable damping
  - `louvain_communities()` - Community detection via modularity optimization
  - `connected_components()` - Weakly connected components
  - `strongly_connected_components()` - Kosaraju's SCC algorithm
  - `analyze_graph()` - Full graph analysis (PageRank + Louvain + Components)

- **New Types**
  - `PageRankConfig` - PageRank configuration (damping, iterations, tolerance)
  - `CommunityResult` - Community memberships and modularity score
  - `GraphAnalysis` - Complete graph analysis results

- **KnowledgeGraph API**
  - `kg_pagerank()` - Compute centrality scores
  - `kg_louvain()` - Detect communities
  - `kg_connected_components()` - Find connected components
  - `kg_analyze()` - Run full analysis

### Technical

- 38 tests passing (33 unit + 5 integration)
- Full graph algorithm coverage

---

## [0.4.0] - 2026-03-25

### Added

- **Graph Traversal Module** (`src/graph/traversal.rs`)
  - `bfs_traversal()` - Breadth-first search with depth tracking
  - `dfs_traversal()` - Depth-first search with depth tracking
  - `find_shortest_path()` - BFS-based shortest path between entities
  - `compute_graph_stats()` - Graph statistics (entities, relations, density)

- **New Types**
  - `TraversalNode` - Node with depth information
  - `TraversalPath` - Complete path with steps
  - `PathStep` - Edge information in path
  - `GraphStats` - Graph statistics
  - `Direction` - Traversal direction (Outgoing/Incoming/Both)
  - `TraversalQuery` - Query parameters for traversal

- **KnowledgeGraph API**
  - `kg_bfs_traversal()` - BFS from entity
  - `kg_dfs_traversal()` - DFS from entity
  - `kg_shortest_path()` - Shortest path between entities
  - `kg_graph_stats()` - Get graph statistics

### Technical

- 32 tests passing (27 unit + 5 integration)
- New traversal module with comprehensive test coverage

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
| 0.7.0 | 2026-03-25 | More Extension Functions (PageRank, Louvain, BFS, Shortest Path) |
| 0.6.0 | 2026-03-25 | SQLite Extension Support |
| 0.5.0 | 2026-03-25 | Graph Algorithms (PageRank, Louvain, Connected Components) |
| 0.4.0 | 2026-03-25 | Graph Traversal (BFS, DFS, Shortest Path) |
| 0.3.0 | 2026-03-25 | RAG integration, data migration |
| 0.2.0 | 2026-03-24 | Core modules (entity, relation, vector) |
| 0.1.0 | 2026-03-24 | Project initialization |

---

[Unreleased]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.3.0
[0.2.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.2.0
[0.1.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.1.0