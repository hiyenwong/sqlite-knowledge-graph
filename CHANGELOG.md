# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

---

## [0.10.2] - 2026-04-01

### Performance

- **Persistent TurboQuant Index** - Eliminated per-query index rebuild in RAG Stage 1
  - Index serialized as JSON BLOB and stored in new `kg_turboquant_cache` table
  - Cache invalidated automatically when `kg_vectors` row count changes
  - Same database with repeated RAG queries now builds the index only once
  - `TurboQuantIndex::to_bytes()` / `from_bytes()` added for BLOB round-trip

### Technical

- New table: `kg_turboquant_cache` (singleton row, `id = 1`)
- 95 tests passing

---

## [0.10.1] - 2026-03-31

### Fixed

- All P0/P1/P2 quality issues resolved
  - Louvain Phase 2 super-node aggregation implemented (`P1-1`)
  - Remaining bare `.unwrap()` calls replaced with `map_err(?)` (`P0-5`)

### Technical

- 95 tests passing

---

## [0.10.0] - 2026-03-31

### Added

- **Paper-driven two-stage RAG Engine** (`src/rag/mod.rs`)
  - Stage 1 (MemRL): TurboQuant ANN fast candidate retrieval
  - Stage 2 (MemRL): exact cosine rerank
  - RAPO: BFS graph-neighbour expansion
  - SuperLocalMemory: quality threshold filtering
  - Memex(RL): context entity BFS attachment
  - `RagEngine`, `RagConfig`, `RagResult` public types

### References

- MemRL (2601.03192), RAPO (2603.02958), Memex (2603.03561)
- SuperLocalMemory (2602.13398), NN-RAG (2511.20333)

### Technical

- 95 tests passing

---

## [0.9.0] - 2026-03-26

### Added

- **Vector Embedding Generation** - Real embeddings with sentence-transformers
  - `EmbeddingGenerator` - Generate embeddings using `all-MiniLM-L6-v2` (384 dimensions)
  - `sqlite-kg embed` CLI command - Generate embeddings for papers and skills
  - Batch processing (100 entities/batch)
  - Incremental mode - Skip entities with existing real embeddings
  - `--force` flag - Regenerate all embeddings

### Changed

- **Search Command Fixed** - Now uses real query embeddings instead of dummy vectors
  - Semantic search similarity scores now in 0.7-0.8+ range (previously 0.05-0.07)
  - Results are highly relevant to query

### Technical

- New module: `src/embed.rs` (~400 lines)
- Python integration via subprocess for sentence-transformers
- 20 new unit tests for embedding functionality
- Integration test with dependency check
- Total: 60 tests passing

### Usage

```bash
# Generate embeddings
sqlite-kg embed --db kg.db

# Generate for papers only
sqlite-kg embed --db kg.db --papers

# Force regenerate all
sqlite-kg embed --db kg.db --force

# Semantic search
sqlite-kg search "brain network" --k 5 --db kg.db
```

### Dependencies

- Requires `sentence-transformers` Python package
- Virtual environment recommended: `python3 -m venv .venv && pip install sentence-transformers`

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
| 0.10.2 | 2026-04-01 | Persistent TurboQuant index (SQLite cache) |
| 0.10.1 | 2026-03-31 | All P0/P1/P2 quality issues resolved |
| 0.10.0 | 2026-03-31 | Paper-driven two-stage RAG engine |
| 0.9.0 | 2026-03-26 | Vector embedding generation (sentence-transformers) |
| 0.8.0 | 2026-03-25 | TurboQuant vector indexing (ANN) |
| 0.7.0 | 2026-03-25 | More extension functions (PageRank, Louvain, BFS, Shortest Path) |
| 0.6.0 | 2026-03-25 | SQLite extension support |
| 0.5.0 | 2026-03-25 | Graph algorithms (PageRank, Louvain, Connected Components) |
| 0.4.0 | 2026-03-25 | Graph traversal (BFS, DFS, Shortest Path) |
| 0.3.0 | 2026-03-25 | RAG integration, data migration |
| 0.2.0 | 2026-03-24 | Core modules (entity, relation, vector) |
| 0.1.0 | 2026-03-24 | Project initialization |

---

[Unreleased]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.10.2...HEAD
[0.10.2]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.10.1...v0.10.2
[0.10.1]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.3.0
[0.2.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.2.0
[0.1.0]: https://github.com/hiyenwong/sqlite-knowledge-graph/releases/tag/v0.1.0