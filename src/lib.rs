//! SQLite-based Knowledge Graph Library
//!
//! This library provides a knowledge graph implementation built on SQLite with support for:
//! - Entities with typed properties
//! - Relations between entities with weights
//! - Vector embeddings for semantic search
//! - Custom SQLite functions for direct SQL operations
//! - RAG (Retrieval-Augmented Generation) query functions
//! - Graph algorithms (PageRank, Louvain, Connected Components)
//!
//! ## SQLite Extension
//!
//! This crate can be compiled as a SQLite loadable extension:
//! ```bash
//! cargo build --release
//! sqlite3 db.db ".load ./target/release/libsqlite_knowledge_graph.dylib"
//! sqlite3 db.db "SELECT kg_version();"
//! ```

pub mod algorithms;
pub mod embed;
pub mod error;
pub mod extension;
pub mod functions;
pub mod graph;
pub mod migrate;
pub mod schema;
pub mod vector;

pub use algorithms::{
    analyze_graph, connected_components, louvain_communities, pagerank, CommunityResult,
    PageRankConfig,
};
pub use embed::{
    check_dependencies, get_entities_needing_embedding, EmbeddingConfig, EmbeddingGenerator,
    EmbeddingStats,
};
pub use error::{Error, Result};
pub use extension::sqlite3_sqlite_knowledge_graph_init;
pub use functions::register_functions;
pub use graph::{Direction, GraphStats, PathStep, TraversalNode, TraversalPath, TraversalQuery};
pub use graph::{Entity, Neighbor, Relation};
pub use migrate::{
    build_relationships, migrate_all, migrate_papers, migrate_skills, MigrationStats,
};
pub use schema::{create_schema, schema_exists};
pub use vector::{cosine_similarity, SearchResult, VectorStore};
pub use vector::{TurboQuantConfig, TurboQuantIndex, TurboQuantStats};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Semantic search result with entity information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultWithEntity {
    pub entity: Entity,
    pub similarity: f32,
}

/// Graph context for an entity (root + neighbors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphContext {
    pub root_entity: Entity,
    pub neighbors: Vec<Neighbor>,
}

/// Hybrid search result combining semantic similarity and graph context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub entity: Entity,
    pub similarity: f32,
    pub context: Option<GraphContext>,
}

/// Knowledge Graph Manager - main entry point for the library.
#[derive(Debug)]
pub struct KnowledgeGraph {
    conn: Connection,
}

impl KnowledgeGraph {
    /// Open a new knowledge graph database connection.
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Create schema if not exists
        if !schema_exists(&conn)? {
            create_schema(&conn)?;
        }

        // Register custom functions
        register_functions(&conn)?;

        Ok(Self { conn })
    }

    /// Open an in-memory knowledge graph (useful for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Create schema
        create_schema(&conn)?;

        // Register custom functions
        register_functions(&conn)?;

        Ok(Self { conn })
    }

    /// Get a reference to the underlying SQLite connection.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Begin a transaction for batch operations.
    pub fn transaction(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(self.conn.unchecked_transaction()?)
    }

    /// Insert an entity into the knowledge graph.
    pub fn insert_entity(&self, entity: &Entity) -> Result<i64> {
        graph::insert_entity(&self.conn, entity)
    }

    /// Get an entity by ID.
    pub fn get_entity(&self, id: i64) -> Result<Entity> {
        graph::get_entity(&self.conn, id)
    }

    /// List entities with optional filtering.
    pub fn list_entities(
        &self,
        entity_type: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<Entity>> {
        graph::list_entities(&self.conn, entity_type, limit)
    }

    /// Update an entity.
    pub fn update_entity(&self, entity: &Entity) -> Result<()> {
        graph::update_entity(&self.conn, entity)
    }

    /// Delete an entity.
    pub fn delete_entity(&self, id: i64) -> Result<()> {
        graph::delete_entity(&self.conn, id)
    }

    /// Insert a relation between entities.
    pub fn insert_relation(&self, relation: &Relation) -> Result<i64> {
        graph::insert_relation(&self.conn, relation)
    }

    /// Get neighbors of an entity using BFS traversal.
    pub fn get_neighbors(&self, entity_id: i64, depth: u32) -> Result<Vec<Neighbor>> {
        graph::get_neighbors(&self.conn, entity_id, depth)
    }

    /// Insert a vector embedding for an entity.
    pub fn insert_vector(&self, entity_id: i64, vector: Vec<f32>) -> Result<()> {
        let store = VectorStore::new();
        store.insert_vector(&self.conn, entity_id, vector)
    }

    /// Search for similar entities using vector embeddings.
    pub fn search_vectors(&self, query: Vec<f32>, k: usize) -> Result<Vec<SearchResult>> {
        let store = VectorStore::new();
        store.search_vectors(&self.conn, query, k)
    }

    // ========== TurboQuant Vector Index ==========

    /// Create a TurboQuant index for fast approximate nearest neighbor search.
    ///
    /// TurboQuant provides:
    /// - Instant indexing (no training required)
    /// - 6x memory compression
    /// - Near-zero accuracy loss
    ///
    /// # Arguments
    /// * `config` - Optional configuration (uses defaults if None)
    ///
    /// # Example
    /// ```ignore
    /// let config = TurboQuantConfig {
    ///     dimension: 384,
    ///     bit_width: 3,
    ///     seed: 42,
    /// };
    /// let mut index = kg.create_turboquant_index(Some(config))?;
    ///
    /// // Add vectors to index
    /// for (entity_id, vector) in all_vectors {
    ///     index.add_vector(entity_id, &vector)?;
    /// }
    ///
    /// // Fast search
    /// let results = index.search(&query_vector, 10)?;
    /// ```
    pub fn create_turboquant_index(
        &self,
        config: Option<TurboQuantConfig>,
    ) -> Result<TurboQuantIndex> {
        let config = config.unwrap_or_default();

        TurboQuantIndex::new(config)
    }

    /// Build a TurboQuant index from all existing vectors in the database.
    /// This is a convenience method that loads all vectors and indexes them.
    pub fn build_turboquant_index(
        &self,
        config: Option<TurboQuantConfig>,
    ) -> Result<TurboQuantIndex> {
        // Get dimension from first vector
        let dimension = self.get_vector_dimension()?.unwrap_or(384);

        let config = config.unwrap_or(TurboQuantConfig {
            dimension,
            bit_width: 3,
            seed: 42,
        });

        let mut index = TurboQuantIndex::new(config)?;

        // Load all vectors
        let vectors = self.load_all_vectors()?;

        for (entity_id, vector) in vectors {
            index.add_vector(entity_id, &vector)?;
        }

        Ok(index)
    }

    /// Get the dimension of stored vectors (if any exist).
    fn get_vector_dimension(&self) -> Result<Option<usize>> {
        let result = self
            .conn
            .query_row("SELECT dimension FROM kg_vectors LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            });

        match result {
            Ok(dim) => Ok(Some(dim as usize)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Load all vectors from the database.
    fn load_all_vectors(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT entity_id, vector, dimension FROM kg_vectors")?;

        let rows = stmt.query_map([], |row| {
            let entity_id: i64 = row.get(0)?;
            let vector_blob: Vec<u8> = row.get(1)?;
            let dimension: i64 = row.get(2)?;

            let mut vector = Vec::with_capacity(dimension as usize);
            for chunk in vector_blob.chunks_exact(4) {
                let bytes: [u8; 4] = chunk.try_into().unwrap();
                vector.push(f32::from_le_bytes(bytes));
            }

            Ok((entity_id, vector))
        })?;

        let mut vectors = Vec::new();
        for row in rows {
            vectors.push(row?);
        }

        Ok(vectors)
    }

    // ========== RAG Query Functions ==========

    /// Semantic search using vector embeddings.
    /// Returns entities sorted by similarity score.
    pub fn kg_semantic_search(
        &self,
        query_embedding: Vec<f32>,
        k: usize,
    ) -> Result<Vec<SearchResultWithEntity>> {
        let results = self.search_vectors(query_embedding, k)?;

        let mut entities_with_results = Vec::new();
        for result in results {
            let entity = self.get_entity(result.entity_id)?;
            entities_with_results.push(SearchResultWithEntity {
                entity,
                similarity: result.similarity,
            });
        }

        Ok(entities_with_results)
    }

    /// Get context around an entity using graph traversal.
    /// Returns neighbors up to the specified depth.
    pub fn kg_get_context(&self, entity_id: i64, depth: u32) -> Result<GraphContext> {
        let root_entity = self.get_entity(entity_id)?;
        let neighbors = self.get_neighbors(entity_id, depth)?;

        Ok(GraphContext {
            root_entity,
            neighbors,
        })
    }

    /// Hybrid search combining semantic search and graph context.
    /// Performs semantic search first, then retrieves context for top-k results.
    pub fn kg_hybrid_search(
        &self,
        _query_text: &str,
        query_embedding: Vec<f32>,
        k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let semantic_results = self.kg_semantic_search(query_embedding, k)?;

        let mut hybrid_results = Vec::new();
        for result in semantic_results.iter() {
            let entity_id = result.entity.id.ok_or(Error::EntityNotFound(0))?;
            let context = self.kg_get_context(entity_id, 1)?; // Depth 1 context

            hybrid_results.push(HybridSearchResult {
                entity: result.entity.clone(),
                similarity: result.similarity,
                context: Some(context),
            });
        }

        Ok(hybrid_results)
    }

    // ========== Graph Traversal Functions ==========

    /// BFS traversal from a starting entity.
    /// Returns all reachable entities within max_depth with depth information.
    pub fn kg_bfs_traversal(
        &self,
        start_id: i64,
        direction: Direction,
        max_depth: u32,
    ) -> Result<Vec<TraversalNode>> {
        let query = TraversalQuery {
            direction,
            max_depth,
            ..Default::default()
        };
        graph::bfs_traversal(&self.conn, start_id, query)
    }

    /// DFS traversal from a starting entity.
    /// Returns all reachable entities within max_depth.
    pub fn kg_dfs_traversal(
        &self,
        start_id: i64,
        direction: Direction,
        max_depth: u32,
    ) -> Result<Vec<TraversalNode>> {
        let query = TraversalQuery {
            direction,
            max_depth,
            ..Default::default()
        };
        graph::dfs_traversal(&self.conn, start_id, query)
    }

    /// Find shortest path between two entities using BFS.
    /// Returns the path with all intermediate steps (if exists).
    pub fn kg_shortest_path(
        &self,
        from_id: i64,
        to_id: i64,
        max_depth: u32,
    ) -> Result<Option<TraversalPath>> {
        graph::find_shortest_path(&self.conn, from_id, to_id, max_depth)
    }

    /// Compute graph statistics.
    pub fn kg_graph_stats(&self) -> Result<GraphStats> {
        graph::compute_graph_stats(&self.conn)
    }

    // ========== Graph Algorithms ==========

    /// Compute PageRank scores for all entities.
    /// Returns a vector of (entity_id, score) sorted by score descending.
    pub fn kg_pagerank(&self, config: Option<PageRankConfig>) -> Result<Vec<(i64, f64)>> {
        algorithms::pagerank(&self.conn, config.unwrap_or_default())
    }

    /// Detect communities using Louvain algorithm.
    /// Returns community memberships and modularity score.
    pub fn kg_louvain(&self) -> Result<CommunityResult> {
        algorithms::louvain_communities(&self.conn)
    }

    /// Find connected components in the graph.
    /// Returns a list of components, each being a list of entity IDs.
    pub fn kg_connected_components(&self) -> Result<Vec<Vec<i64>>> {
        algorithms::connected_components(&self.conn)
    }

    /// Run full graph analysis (PageRank + Louvain + Connected Components).
    pub fn kg_analyze(&self) -> Result<algorithms::GraphAnalysis> {
        algorithms::analyze_graph(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        assert!(schema_exists(kg.connection()).unwrap());
    }

    #[test]
    fn test_crud_operations() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();

        // Create entity
        let mut entity = Entity::new("paper", "Test Paper");
        entity.set_property("author", serde_json::json!("John Doe"));
        let id = kg.insert_entity(&entity).unwrap();

        // Read entity
        let retrieved = kg.get_entity(id).unwrap();
        assert_eq!(retrieved.name, "Test Paper");

        // List entities
        let entities = kg.list_entities(Some("paper"), None).unwrap();
        assert_eq!(entities.len(), 1);

        // Update entity
        let mut updated = retrieved.clone();
        updated.set_property("year", serde_json::json!(2024));
        kg.update_entity(&updated).unwrap();

        // Delete entity
        kg.delete_entity(id).unwrap();
        let entities = kg.list_entities(None, None).unwrap();
        assert_eq!(entities.len(), 0);
    }

    #[test]
    fn test_graph_traversal() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();

        // Create entities
        let id1 = kg.insert_entity(&Entity::new("paper", "Paper 1")).unwrap();
        let id2 = kg.insert_entity(&Entity::new("paper", "Paper 2")).unwrap();
        let id3 = kg.insert_entity(&Entity::new("paper", "Paper 3")).unwrap();

        // Create relations
        kg.insert_relation(&Relation::new(id1, id2, "cites", 0.8).unwrap())
            .unwrap();
        kg.insert_relation(&Relation::new(id2, id3, "cites", 0.9).unwrap())
            .unwrap();

        // Get neighbors depth 1
        let neighbors = kg.get_neighbors(id1, 1).unwrap();
        assert_eq!(neighbors.len(), 1);

        // Get neighbors depth 2
        let neighbors = kg.get_neighbors(id1, 2).unwrap();
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn test_vector_search() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();

        // Create entities
        let id1 = kg.insert_entity(&Entity::new("paper", "Paper 1")).unwrap();
        let id2 = kg.insert_entity(&Entity::new("paper", "Paper 2")).unwrap();

        // Insert vectors
        kg.insert_vector(id1, vec![1.0, 0.0, 0.0]).unwrap();
        kg.insert_vector(id2, vec![0.0, 1.0, 0.0]).unwrap();

        // Search
        let results = kg.search_vectors(vec![1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entity_id, id1);
    }
}
