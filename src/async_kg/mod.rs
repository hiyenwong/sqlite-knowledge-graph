//! Async wrapper around [`KnowledgeGraph`].
//!
//! Enabled with the `async` Cargo feature. All blocking SQLite operations are
//! dispatched to a `tokio::task::spawn_blocking` thread, keeping the async
//! executor thread free.
//!
//! # Usage
//!
//! ```ignore
//! use sqlite_knowledge_graph::{AsyncKnowledgeGraph, Entity};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let kg = AsyncKnowledgeGraph::open_in_memory_sync()?;
//!     let entity = Entity::new("paper", "Async Test");
//!     let id = kg.insert_entity(entity).await?;
//!     let retrieved = kg.get_entity(id).await?;
//!     println!("{:?}", retrieved);
//!     Ok(())
//! }
//! ```
//!
//! # Thread Safety
//!
//! Internally the [`KnowledgeGraph`] is wrapped in `Arc<Mutex<_>>`. All async
//! methods acquire the lock inside the `spawn_blocking` closure — the lock is
//! released before any `.await` point, so no `MutexGuard` ever crosses an
//! await boundary.
//!
//! For read-heavy concurrent workloads, consider opening multiple
//! `AsyncKnowledgeGraph` instances on the same file — SQLite WAL mode supports
//! concurrent readers.

pub mod embed;

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::algorithms::GraphAnalysis;
use crate::error::{Error, Result};
use crate::{
    CommunityResult, Direction, DotConfig, GraphContext, GraphStats, HigherOrderNeighbor,
    HigherOrderPath, HybridSearchResult, Hyperedge, KnowledgeGraph, Neighbor, PageRankConfig,
    SearchResult, SearchResultWithEntity, TraversalNode, TraversalPath,
};
use crate::{D3ExportGraph, Entity, Relation};

/// Async wrapper around [`KnowledgeGraph`].
///
/// Every method dispatches the blocking SQLite work to a dedicated OS thread
/// via [`tokio::task::spawn_blocking`], leaving the async executor responsive.
///
/// # Note on argument ownership
///
/// Unlike the sync [`KnowledgeGraph`] which borrows (`&Entity`), the async
/// methods take **owned** values. This is required because the closure passed
/// to `spawn_blocking` must be `'static`.
pub struct AsyncKnowledgeGraph {
    inner: Arc<Mutex<KnowledgeGraph>>,
}

// The Arc<Mutex<KnowledgeGraph>> can be sent across threads safely.
// KnowledgeGraph itself is not Send (rusqlite::Connection), but the Mutex
// wrapper ensures exclusive access.
unsafe impl Send for AsyncKnowledgeGraph {}
unsafe impl Sync for AsyncKnowledgeGraph {}

/// Dispatch a method call to a `spawn_blocking` thread.
///
/// Clones the `Arc`, moves it into the closure, locks the `Mutex` inside the
/// closure (on the blocking thread), calls the method, and awaits the result.
macro_rules! dispatch {
    ($self:expr, $method:ident $(, $arg:expr)*) => {{
        let inner = Arc::clone(&$self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.$method($($arg),*)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }};
}

impl AsyncKnowledgeGraph {
    // ── Construction ──────────────────────────────────────────────────────

    /// Open an async knowledge graph backed by a file on disk.
    ///
    /// This call blocks briefly (opens the SQLite connection) but is typically
    /// called once at startup so the blocking cost is acceptable.
    pub fn open_sync<P: AsRef<Path>>(path: P) -> Result<Self> {
        let kg = KnowledgeGraph::open(path)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(kg)),
        })
    }

    /// Open an in-memory async knowledge graph (useful for testing).
    pub fn open_in_memory_sync() -> Result<Self> {
        let kg = KnowledgeGraph::open_in_memory()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(kg)),
        })
    }

    /// Wrap an existing synchronous [`KnowledgeGraph`] in an async adapter.
    pub fn from_sync(kg: KnowledgeGraph) -> Self {
        Self {
            inner: Arc::new(Mutex::new(kg)),
        }
    }

    /// Return a clone of the inner `Arc<Mutex<KnowledgeGraph>>` for operations
    /// not yet exposed through the async API.
    pub fn inner(&self) -> Arc<Mutex<KnowledgeGraph>> {
        Arc::clone(&self.inner)
    }

    // ── Entity CRUD ───────────────────────────────────────────────────────

    /// Insert an entity and return its new ID.
    pub async fn insert_entity(&self, entity: Entity) -> Result<i64> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.insert_entity(&entity)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Get an entity by ID.
    pub async fn get_entity(&self, id: i64) -> Result<Entity> {
        dispatch!(self, get_entity, id)
    }

    /// List entities with optional type filter and limit.
    pub async fn list_entities(
        &self,
        entity_type: Option<String>,
        limit: Option<i64>,
    ) -> Result<Vec<Entity>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.list_entities(entity_type.as_deref(), limit)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Update an entity.
    pub async fn update_entity(&self, entity: Entity) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.update_entity(&entity)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Delete an entity by ID.
    pub async fn delete_entity(&self, id: i64) -> Result<()> {
        dispatch!(self, delete_entity, id)
    }

    // ── Relation CRUD ─────────────────────────────────────────────────────

    /// Insert a relation between two entities and return its new ID.
    pub async fn insert_relation(&self, relation: Relation) -> Result<i64> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.insert_relation(&relation)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Get neighbours of an entity up to `depth` hops.
    pub async fn get_neighbors(&self, entity_id: i64, depth: u32) -> Result<Vec<Neighbor>> {
        dispatch!(self, get_neighbors, entity_id, depth)
    }

    // ── Hyperedge CRUD ────────────────────────────────────────────────────

    /// Insert a hyperedge and return its new ID.
    pub async fn insert_hyperedge(&self, hyperedge: Hyperedge) -> Result<i64> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.insert_hyperedge(&hyperedge)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Get a hyperedge by ID.
    pub async fn get_hyperedge(&self, id: i64) -> Result<Hyperedge> {
        dispatch!(self, get_hyperedge, id)
    }

    /// List hyperedges with optional filters.
    pub async fn list_hyperedges(
        &self,
        hyperedge_type: Option<String>,
        min_arity: Option<usize>,
        max_arity: Option<usize>,
        limit: Option<i64>,
    ) -> Result<Vec<Hyperedge>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.list_hyperedges(hyperedge_type.as_deref(), min_arity, max_arity, limit)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Update a hyperedge.
    pub async fn update_hyperedge(&self, hyperedge: Hyperedge) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.update_hyperedge(&hyperedge)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Delete a hyperedge by ID.
    pub async fn delete_hyperedge(&self, id: i64) -> Result<()> {
        dispatch!(self, delete_hyperedge, id)
    }

    /// Get higher-order neighbours connected through hyperedges.
    pub async fn get_higher_order_neighbors(
        &self,
        entity_id: i64,
        min_arity: Option<usize>,
        max_arity: Option<usize>,
    ) -> Result<Vec<HigherOrderNeighbor>> {
        dispatch!(self, get_higher_order_neighbors, entity_id, min_arity, max_arity)
    }

    /// Get all hyperedges an entity participates in.
    pub async fn get_entity_hyperedges(&self, entity_id: i64) -> Result<Vec<Hyperedge>> {
        dispatch!(self, get_entity_hyperedges, entity_id)
    }

    // ── Vector search ─────────────────────────────────────────────────────

    /// Store a vector embedding for an entity.
    pub async fn insert_vector(&self, entity_id: i64, vector: Vec<f32>) -> Result<()> {
        dispatch!(self, insert_vector, entity_id, vector)
    }

    /// Exact nearest-neighbour search over stored vectors.
    pub async fn search_vectors(&self, query: Vec<f32>, k: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, search_vectors, query, k)
    }

    /// Semantic search — returns top-k entities by vector similarity.
    pub async fn kg_semantic_search(
        &self,
        query_embedding: Vec<f32>,
        k: usize,
    ) -> Result<Vec<SearchResultWithEntity>> {
        dispatch!(self, kg_semantic_search, query_embedding, k)
    }

    /// Hybrid search combining semantic and graph context.
    pub async fn kg_hybrid_search(
        &self,
        query_text: String,
        query_embedding: Vec<f32>,
        k: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.kg_hybrid_search(&query_text, query_embedding, k)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }

    /// Find entities similar to the given entity by vector cosine similarity.
    pub async fn kg_similar_entities(
        &self,
        entity_id: i64,
        k: usize,
    ) -> Result<Vec<SearchResultWithEntity>> {
        dispatch!(self, kg_similar_entities, entity_id, k)
    }

    /// Get graph context (root + depth-1 neighbours) for an entity.
    pub async fn kg_get_context(&self, entity_id: i64, depth: u32) -> Result<GraphContext> {
        dispatch!(self, kg_get_context, entity_id, depth)
    }

    /// Find related entities above a relation-weight threshold.
    pub async fn kg_find_related(
        &self,
        entity_id: i64,
        threshold: f64,
    ) -> Result<Vec<(Entity, f64)>> {
        dispatch!(self, kg_find_related, entity_id, threshold)
    }

    // ── Graph traversal ───────────────────────────────────────────────────

    /// BFS traversal from `start_id` up to `max_depth` hops.
    pub async fn kg_bfs_traversal(
        &self,
        start_id: i64,
        direction: Direction,
        max_depth: u32,
    ) -> Result<Vec<TraversalNode>> {
        dispatch!(self, kg_bfs_traversal, start_id, direction, max_depth)
    }

    /// DFS traversal from `start_id` up to `max_depth` hops.
    pub async fn kg_dfs_traversal(
        &self,
        start_id: i64,
        direction: Direction,
        max_depth: u32,
    ) -> Result<Vec<TraversalNode>> {
        dispatch!(self, kg_dfs_traversal, start_id, direction, max_depth)
    }

    /// Shortest path (BFS) between two entities.
    pub async fn kg_shortest_path(
        &self,
        from_id: i64,
        to_id: i64,
        max_depth: u32,
    ) -> Result<Option<TraversalPath>> {
        dispatch!(self, kg_shortest_path, from_id, to_id, max_depth)
    }

    /// Graph statistics (entity count, relation count, density, etc.).
    pub async fn kg_graph_stats(&self) -> Result<GraphStats> {
        dispatch!(self, kg_graph_stats)
    }

    // ── Graph algorithms ──────────────────────────────────────────────────

    /// PageRank scores for all entities, sorted descending.
    pub async fn kg_pagerank(
        &self,
        config: Option<PageRankConfig>,
    ) -> Result<Vec<(i64, f64)>> {
        dispatch!(self, kg_pagerank, config)
    }

    /// Louvain community detection.
    pub async fn kg_louvain(&self) -> Result<CommunityResult> {
        dispatch!(self, kg_louvain)
    }

    /// Connected components — list of entity-ID groups.
    pub async fn kg_connected_components(&self) -> Result<Vec<Vec<i64>>> {
        dispatch!(self, kg_connected_components)
    }

    /// Full graph analysis: PageRank + Louvain + connected components.
    pub async fn kg_analyze(&self) -> Result<GraphAnalysis> {
        dispatch!(self, kg_analyze)
    }

    // ── Higher-order traversal ────────────────────────────────────────────

    /// BFS through hyperedges from `start_id`.
    pub async fn kg_higher_order_bfs(
        &self,
        start_id: i64,
        max_depth: u32,
        min_arity: Option<usize>,
    ) -> Result<Vec<TraversalNode>> {
        dispatch!(self, kg_higher_order_bfs, start_id, max_depth, min_arity)
    }

    /// Shortest path through hyperedges between two entities.
    pub async fn kg_higher_order_shortest_path(
        &self,
        from_id: i64,
        to_id: i64,
        max_depth: u32,
    ) -> Result<Option<HigherOrderPath>> {
        dispatch!(self, kg_higher_order_shortest_path, from_id, to_id, max_depth)
    }

    // ── Export ────────────────────────────────────────────────────────────

    /// Export graph as D3.js JSON.
    pub async fn export_json(&self) -> Result<D3ExportGraph> {
        dispatch!(self, export_json)
    }

    /// Export graph as DOT (Graphviz) string.
    pub async fn export_dot(&self, config: DotConfig) -> Result<String> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let kg = inner
                .lock()
                .map_err(|e| Error::TaskPanicked(format!("mutex poisoned: {e}")))?;
            kg.export_dot(&config)
        })
        .await
        .map_err(|e| Error::TaskPanicked(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Entity;

    #[tokio::test]
    async fn test_open_in_memory() {
        let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();
        // Verify we can access the inner KnowledgeGraph
        let _inner = kg.inner();
    }

    #[tokio::test]
    async fn test_crud_roundtrip() {
        let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

        let entity = Entity::new("paper", "Async Test Paper");
        let id = kg.insert_entity(entity).await.unwrap();

        let retrieved = kg.get_entity(id).await.unwrap();
        assert_eq!(retrieved.name, "Async Test Paper");
        assert_eq!(retrieved.entity_type, "paper");

        let list = kg.list_entities(Some("paper".into()), None).await.unwrap();
        assert_eq!(list.len(), 1);

        let mut updated = retrieved.clone();
        updated.name = "Updated Async Paper".to_string();
        kg.update_entity(updated).await.unwrap();

        let after_update = kg.get_entity(id).await.unwrap();
        assert_eq!(after_update.name, "Updated Async Paper");

        kg.delete_entity(id).await.unwrap();
        let after_delete = kg.get_entity(id).await;
        assert!(after_delete.is_err());
    }

    #[tokio::test]
    async fn test_insert_relation() {
        let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

        let id1 = kg.insert_entity(Entity::new("node", "A")).await.unwrap();
        let id2 = kg.insert_entity(Entity::new("node", "B")).await.unwrap();

        use crate::graph::Relation;
        let relation = Relation::new(id1, id2, "links_to", 0.9).unwrap();
        let _rel_id = kg.insert_relation(relation).await.unwrap();

        let neighbors = kg.get_neighbors(id1, 1).await.unwrap();
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].entity.id.unwrap(), id2);
    }

    #[tokio::test]
    async fn test_from_sync() {
        let sync_kg = KnowledgeGraph::open_in_memory().unwrap();
        let async_kg = AsyncKnowledgeGraph::from_sync(sync_kg);
        let entity = Entity::new("test", "FromSync");
        let id = async_kg.insert_entity(entity).await.unwrap();
        assert!(id > 0);
    }

    #[tokio::test]
    async fn test_into_async_conversion() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        let async_kg = kg.into_async();
        let entity = Entity::new("test", "IntoAsync");
        let id = async_kg.insert_entity(entity).await.unwrap();
        assert!(id > 0);
    }
}
