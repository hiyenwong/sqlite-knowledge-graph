//! Paper-driven two-stage RAG engine.
//!
//! Pipeline (per-query):
//! ```text
//! query: &str
//!   → Embedder::embed()                      [plug-in]
//!   → [Stage 1 · MemRL]  TurboQuantIndex ANN (top_k_candidates)
//!   → [Stage 2 · MemRL]  exact cosine rerank (top_k_rerank)
//!   → [RAPO]             BFS graph expansion + vector score
//!   → [combined score]   vector_weight·v + graph_weight·g
//!   → [SuperLocalMemory] quality filter (min thresholds)
//!   → sort desc, take k
//!   → [Memex(RL)]        context BFS for each result (max_context_entities)
//! ```
//!
//! References:
//! - MemRL (2601.03192): two-stage ANN → exact rerank
//! - RAPO  (2603.02958): graph-neighbour expansion
//! - Memex (2603.03561): context-entity sizing
//! - SuperLocalMemory (2602.13398): quality threshold filtering
//! - NN-RAG (2511.20333): retrieval quality over quantity

pub mod embedder;
mod error;

pub use embedder::Embedder;
pub use error::RagError;

use crate::error::Result;
use crate::graph::{get_neighbors, Entity};
use crate::vector::{cosine_similarity, TurboQuantConfig, TurboQuantIndex, VectorStore};
use rusqlite::Connection;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// One result row from the RAG engine.
#[derive(Debug, Clone)]
pub struct RagResult {
    /// The matched entity.
    pub entity: Entity,
    /// Exact cosine similarity score in [0, 1].
    pub vector_score: f64,
    /// Graph connectivity score in [0, 1] (fraction of pool that is a neighbour).
    pub graph_score: f64,
    /// Final weighted score: `vector_weight·v + graph_weight·g`.
    pub combined_score: f64,
    /// Context neighbours collected by BFS (Memex(RL) sizing).
    pub context_entities: Vec<Entity>,
}

/// Configuration knobs for the RAG pipeline.
/// All fields have defaults tuned to the paper recommendations.
#[derive(Debug, Clone)]
pub struct RagConfig {
    // ── Scoring ──────────────────────────────────────────────────────────────
    /// Weight applied to the vector (semantic) score. Default: 0.6.
    pub vector_weight: f64,
    /// Weight applied to the graph connectivity score. Default: 0.4.
    pub graph_weight: f64,

    // ── MemRL two-stage retrieval ─────────────────────────────────────────────
    /// Stage-1: how many ANN candidates to fetch from TurboQuant. Default: 50.
    pub top_k_candidates: usize,
    /// Stage-2: how many candidates survive after exact rerank. Default: 20.
    pub top_k_rerank: usize,

    // ── RAPO graph expansion ──────────────────────────────────────────────────
    /// Whether to expand candidates via BFS neighbours. Default: true.
    pub enable_graph_expansion: bool,
    /// BFS depth for graph-score neighbour collection. Default: 1.
    pub graph_depth: u32,

    // ── Memex(RL) context sizing ──────────────────────────────────────────────
    /// BFS depth for context collection. Default: 2.
    pub context_depth: u32,
    /// Maximum context entities attached to each result. Default: 5.
    pub max_context_entities: usize,

    // ── SuperLocalMemory quality thresholds ───────────────────────────────────
    /// Minimum vector score for a candidate to survive. Default: 0.0 (off).
    pub min_vector_score: f32,
    /// Minimum combined score for a result to survive. Default: 0.0 (off).
    pub min_combined_score: f64,

    // ── TurboQuant index ─────────────────────────────────────────────────────
    /// Vector dimension; set to match your embedding model. Default: 384.
    pub vector_dimension: usize,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.6,
            graph_weight: 0.4,
            top_k_candidates: 50,
            top_k_rerank: 20,
            enable_graph_expansion: true,
            graph_depth: 1,
            context_depth: 2,
            max_context_entities: 5,
            min_vector_score: 0.0,
            min_combined_score: 0.0,
            vector_dimension: 384,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RagEngine
// ─────────────────────────────────────────────────────────────────────────────

/// Hybrid RAG engine backed by SQLite.
pub struct RagEngine {
    config: RagConfig,
}

impl RagEngine {
    pub fn new(config: RagConfig) -> Self {
        Self { config }
    }

    /// Full hybrid search.
    ///
    /// # Arguments
    /// * `conn`   – active SQLite connection (read/write access required)
    /// * `embedder` – embedding backend (see `embedder::SubprocessEmbedder`)
    /// * `query`  – raw query text
    /// * `k`      – how many results to return
    pub fn search(
        &self,
        conn: &Connection,
        embedder: &dyn Embedder,
        query: &str,
        k: usize,
    ) -> Result<Vec<RagResult>> {
        // ── 0. Embed query ───────────────────────────────────────────────────
        let query_vec = embedder.embed(query)?;

        // ── Stage 1 · MemRL – fast ANN via TurboQuant ───────────────────────
        let ann_candidates = self.stage1_ann(conn, &query_vec)?;

        if ann_candidates.is_empty() {
            return Ok(Vec::new());
        }

        // ── Stage 2 · MemRL – exact cosine rerank ───────────────────────────
        let mut reranked = self.stage2_rerank(conn, &query_vec, ann_candidates)?;
        reranked.truncate(self.config.top_k_rerank);

        // ── RAPO – expand with graph neighbours ─────────────────────────────
        let mut pool: HashMap<i64, f32> = reranked.into_iter().collect();
        if self.config.enable_graph_expansion {
            self.rapo_expand(conn, &query_vec, &mut pool)?;
        }

        // ── Score & filter (SuperLocalMemory) ───────────────────────────────
        let pool_size = pool.len();
        let mut scored = self.score_and_filter(conn, &pool, pool_size)?;

        // Sort by combined_score descending, take top k
        scored.sort_by(|a, b| b.combined_score.partial_cmp(&a.combined_score).unwrap());
        scored.truncate(k);

        // ── Memex(RL) – attach context neighbours ───────────────────────────
        for result in &mut scored {
            let entity_id = result.entity.id.unwrap_or(0);
            result.context_entities = self.collect_context(conn, entity_id, &pool)?;
        }

        Ok(scored)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Private helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Stage 1: ANN via TurboQuant.  Returns (entity_id, approx_score) pairs.
    ///
    /// The TurboQuant index is persisted in `kg_turboquant_cache` and only
    /// rebuilt when the number of vectors in `kg_vectors` has changed.
    fn stage1_ann(&self, conn: &Connection, query_vec: &[f32]) -> Result<Vec<(i64, f32)>> {
        let vector_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM kg_vectors", [], |r| r.get(0))?;

        if vector_count == 0 {
            return Ok(Vec::new());
        }

        // Try to load a valid cached index first.
        let cached = load_turboquant_cache(conn, vector_count)?;
        let index = match cached {
            Some(idx) => idx,
            None => {
                // Cache miss or stale — rebuild from kg_vectors.
                let all_vectors = load_all_vectors(conn)?;
                let dim = all_vectors[0].1.len();
                let config = TurboQuantConfig {
                    dimension: dim,
                    bit_width: 3,
                    seed: 42,
                };
                let mut idx = TurboQuantIndex::new(config)?;
                for (entity_id, vec) in &all_vectors {
                    idx.add_vector(*entity_id, vec)?;
                }
                save_turboquant_cache(conn, &idx, vector_count)?;
                idx
            }
        };

        let k = self.config.top_k_candidates.min(vector_count as usize);
        index.search(query_vec, k)
    }

    /// Stage 2: exact cosine rerank.
    fn stage2_rerank(
        &self,
        conn: &Connection,
        query_vec: &[f32],
        candidates: Vec<(i64, f32)>,
    ) -> Result<Vec<(i64, f32)>> {
        let store = VectorStore::new();
        let mut scored: Vec<(i64, f32)> = Vec::with_capacity(candidates.len());

        for (entity_id, approx) in candidates {
            // SuperLocalMemory: drop if even the ANN score is below threshold
            if approx < self.config.min_vector_score {
                continue;
            }
            match store.get_vector(conn, entity_id) {
                Ok(vec) => {
                    let exact = cosine_similarity(query_vec, &vec);
                    if exact >= self.config.min_vector_score {
                        scored.push((entity_id, exact));
                    }
                }
                Err(_) => {
                    // entity_id no longer has a vector – skip silently
                }
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(scored)
    }

    /// RAPO: BFS expand candidate pool with graph neighbours,
    /// computing their vector scores on the fly.
    fn rapo_expand(
        &self,
        conn: &Connection,
        query_vec: &[f32],
        pool: &mut HashMap<i64, f32>,
    ) -> Result<()> {
        let store = VectorStore::new();
        let seeds: Vec<i64> = pool.keys().copied().collect();

        for seed_id in seeds {
            let neighbours = match get_neighbors(conn, seed_id, self.config.graph_depth) {
                Ok(n) => n,
                Err(_) => continue,
            };

            for nbr in neighbours {
                let nbr_id = match nbr.entity.id {
                    Some(id) => id,
                    None => continue,
                };

                if pool.contains_key(&nbr_id) {
                    continue;
                }

                // Score the new candidate
                if let Ok(vec) = store.get_vector(conn, nbr_id) {
                    let score = cosine_similarity(query_vec, &vec);
                    if score >= self.config.min_vector_score {
                        pool.insert(nbr_id, score);
                    }
                }
            }
        }

        Ok(())
    }

    /// Compute graph score, combined score, apply SuperLocalMemory filter,
    /// and build partial RagResult (context filled later).
    fn score_and_filter(
        &self,
        conn: &Connection,
        pool: &HashMap<i64, f32>,
        pool_size: usize,
    ) -> Result<Vec<RagResult>> {
        let mut results = Vec::new();

        for (&entity_id, &v_score) in pool {
            let vector_score = v_score as f64;

            // Graph score: fraction of pool that is a direct neighbour
            let graph_score = if pool_size > 1 {
                let neighbours = get_neighbors(conn, entity_id, 1).unwrap_or_default();
                let overlap = neighbours
                    .iter()
                    .filter(|n| {
                        n.entity
                            .id
                            .map(|id| pool.contains_key(&id))
                            .unwrap_or(false)
                    })
                    .count();
                overlap as f64 / (pool_size - 1) as f64
            } else {
                0.0
            };

            let combined_score =
                self.config.vector_weight * vector_score + self.config.graph_weight * graph_score;

            // SuperLocalMemory quality filter
            if combined_score < self.config.min_combined_score {
                continue;
            }

            let entity = match crate::graph::get_entity(conn, entity_id) {
                Ok(e) => e,
                Err(_) => continue,
            };

            results.push(RagResult {
                entity,
                vector_score,
                graph_score,
                combined_score,
                context_entities: Vec::new(), // filled in next pass
            });
        }

        Ok(results)
    }

    /// Memex(RL): collect context neighbours for a result entity via BFS,
    /// prioritising entities already in the retrieval pool.
    fn collect_context(
        &self,
        conn: &Connection,
        entity_id: i64,
        pool: &HashMap<i64, f32>,
    ) -> Result<Vec<Entity>> {
        let neighbours = match get_neighbors(conn, entity_id, self.config.context_depth) {
            Ok(n) => n,
            Err(_) => return Ok(Vec::new()),
        };

        // Sort: pool members first (high relevance), then by graph-BFS order
        let mut in_pool: Vec<Entity> = Vec::new();
        let mut not_in_pool: Vec<Entity> = Vec::new();

        for nbr in neighbours {
            if let Some(id) = nbr.entity.id {
                if pool.contains_key(&id) {
                    in_pool.push(nbr.entity);
                } else {
                    not_in_pool.push(nbr.entity);
                }
            }
        }

        in_pool.extend(not_in_pool);
        in_pool.truncate(self.config.max_context_entities);
        Ok(in_pool)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Utility
// ─────────────────────────────────────────────────────────────────────────────

/// Load a TurboQuant index from the SQLite cache if it is still valid.
///
/// Returns `None` if no cache row exists or the cached vector count does not
/// match `current_count` (meaning the index is stale).
fn load_turboquant_cache(conn: &Connection, current_count: i64) -> Result<Option<TurboQuantIndex>> {
    let mut stmt =
        conn.prepare("SELECT index_blob, vector_count FROM kg_turboquant_cache WHERE id = 1")?;

    let result = stmt.query_row([], |row| {
        let blob: Vec<u8> = row.get(0)?;
        let cached_count: i64 = row.get(1)?;
        Ok((blob, cached_count))
    });

    match result {
        Ok((blob, cached_count)) if cached_count == current_count => {
            let index = TurboQuantIndex::from_bytes(&blob)
                .map_err(|e| crate::error::Error::Other(e.to_string()))?;
            Ok(Some(index))
        }
        Ok(_) | Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Persist a TurboQuant index into `kg_turboquant_cache` (upsert).
fn save_turboquant_cache(
    conn: &Connection,
    index: &TurboQuantIndex,
    vector_count: i64,
) -> Result<()> {
    let blob = index
        .to_bytes()
        .map_err(|e| crate::error::Error::Other(e.to_string()))?;
    conn.execute(
        "INSERT INTO kg_turboquant_cache (id, index_blob, vector_count) \
         VALUES (1, ?1, ?2) \
         ON CONFLICT(id) DO UPDATE SET index_blob = excluded.index_blob, \
                                       vector_count = excluded.vector_count",
        rusqlite::params![blob, vector_count],
    )?;
    Ok(())
}

fn load_all_vectors(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>> {
    let mut stmt = conn.prepare("SELECT entity_id, vector, dimension FROM kg_vectors")?;

    let rows = stmt.query_map([], |row| {
        let entity_id: i64 = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        let dim: i64 = row.get(2)?;

        let mut vec = Vec::with_capacity(dim as usize);
        for chunk in blob.chunks_exact(4) {
            vec.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }

        Ok((entity_id, vec))
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::entity::{insert_entity, Entity};
    use crate::graph::relation::{insert_relation, Relation};
    use crate::rag::embedder::FixedEmbedder;
    use crate::vector::VectorStore;
    use rusqlite::Connection;

    fn setup(dim: usize) -> (Connection, Vec<i64>) {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let e1 = insert_entity(&conn, &Entity::new("doc", "Doc A")).unwrap();
        let e2 = insert_entity(&conn, &Entity::new("doc", "Doc B")).unwrap();
        let e3 = insert_entity(&conn, &Entity::new("doc", "Doc C")).unwrap();

        let store = VectorStore::new();
        // e1 is very similar to query [1, 0, …]
        let mut v1 = vec![0.0f32; dim];
        v1[0] = 1.0;
        store.insert_vector(&conn, e1, v1).unwrap();

        // e2 is orthogonal to query
        let mut v2 = vec![0.0f32; dim];
        v2[1] = 1.0;
        store.insert_vector(&conn, e2, v2).unwrap();

        // e3 is somewhat similar
        let mut v3 = vec![0.0f32; dim];
        v3[0] = 0.8;
        v3[1] = 0.6;
        store.insert_vector(&conn, e3, v3).unwrap();

        // e1 → e2 (weak link); e1 → e3 (strong link)
        insert_relation(&conn, &Relation::new(e1, e2, "related", 0.3).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(e1, e3, "related", 0.9).unwrap()).unwrap();

        (conn, vec![e1, e2, e3])
    }

    #[test]
    fn test_basic_search() {
        let dim = 4;
        let (conn, ids) = setup(dim);

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;

        let embedder = FixedEmbedder(query);
        let engine = RagEngine::new(RagConfig {
            vector_dimension: dim,
            top_k_candidates: 10,
            top_k_rerank: 5,
            ..Default::default()
        });

        let results = engine.search(&conn, &embedder, "test query", 2).unwrap();
        assert!(!results.is_empty(), "should return at least one result");

        // e1 must be the top result (similarity = 1.0)
        assert_eq!(results[0].entity.id, Some(ids[0]));
        assert!((results[0].vector_score - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let embedder = FixedEmbedder(vec![1.0, 0.0, 0.0]);
        let engine = RagEngine::new(RagConfig::default());

        let results = engine.search(&conn, &embedder, "anything", 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_graph_expansion() {
        // Verify that RAPO brings in neighbours that were not in the ANN results
        let dim = 4;
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let store = VectorStore::new();

        // e1 very similar to query; e2 orthogonal; e1→e2 link
        let e1 = insert_entity(&conn, &Entity::new("doc", "A")).unwrap();
        let e2 = insert_entity(&conn, &Entity::new("doc", "B")).unwrap();

        let mut v1 = vec![0.0f32; dim];
        v1[0] = 1.0;
        store.insert_vector(&conn, e1, v1).unwrap();

        let mut v2 = vec![0.0f32; dim];
        v2[1] = 1.0;
        store.insert_vector(&conn, e2, v2).unwrap();

        insert_relation(&conn, &Relation::new(e1, e2, "link", 1.0).unwrap()).unwrap();

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;

        let embedder = FixedEmbedder(query);
        let engine = RagEngine::new(RagConfig {
            vector_dimension: dim,
            top_k_candidates: 1, // only ANN fetches e1; RAPO adds e2
            top_k_rerank: 1,
            enable_graph_expansion: true,
            ..Default::default()
        });

        let results = engine.search(&conn, &embedder, "q", 5).unwrap();
        let ids: Vec<i64> = results.iter().filter_map(|r| r.entity.id).collect();
        assert!(ids.contains(&e1));
        assert!(ids.contains(&e2), "RAPO should expand to e2");
    }

    #[test]
    fn test_context_attached() {
        let dim = 4;
        let (conn, ids) = setup(dim);

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;

        let embedder = FixedEmbedder(query);
        let engine = RagEngine::new(RagConfig {
            vector_dimension: dim,
            context_depth: 1,
            max_context_entities: 3,
            ..Default::default()
        });

        let results = engine.search(&conn, &embedder, "q", 3).unwrap();

        // e1's result should have context neighbours (e2 and e3)
        let e1_result = results.iter().find(|r| r.entity.id == Some(ids[0]));
        assert!(e1_result.is_some());
        let ctx = &e1_result.unwrap().context_entities;
        assert!(!ctx.is_empty(), "e1 should have context neighbours");
    }

    // ── TurboQuant cache tests ────────────────────────────────────────────────

    #[test]
    fn test_cache_written_on_first_query() {
        let dim = 4;
        let (conn, _ids) = setup(dim);

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;
        let embedder = FixedEmbedder(query);
        let engine = RagEngine::new(RagConfig {
            vector_dimension: dim,
            top_k_candidates: 10,
            top_k_rerank: 5,
            ..Default::default()
        });

        engine.search(&conn, &embedder, "q", 2).unwrap();

        // Cache row must exist after the first search
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_turboquant_cache WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "cache row should be created after first query");
    }

    #[test]
    fn test_cache_hit_on_second_query() {
        let dim = 4;
        let (conn, _ids) = setup(dim);

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;
        let embedder = FixedEmbedder(query);
        let engine = RagEngine::new(RagConfig {
            vector_dimension: dim,
            top_k_candidates: 10,
            top_k_rerank: 5,
            ..Default::default()
        });

        let r1 = engine.search(&conn, &embedder, "q", 2).unwrap();
        let r2 = engine.search(&conn, &embedder, "q", 2).unwrap();

        // Both searches return the same top entity
        assert_eq!(
            r1[0].entity.id, r2[0].entity.id,
            "cache hit should return identical results"
        );
    }

    #[test]
    fn test_cache_invalidated_after_new_vector() {
        let dim = 4;
        let (conn, _ids) = setup(dim);

        let mut query = vec![0.0f32; dim];
        query[0] = 1.0;
        let embedder = FixedEmbedder(query);
        let engine = RagEngine::new(RagConfig {
            vector_dimension: dim,
            top_k_candidates: 10,
            top_k_rerank: 5,
            ..Default::default()
        });

        // First search — writes cache with vector_count = 3
        engine.search(&conn, &embedder, "q", 2).unwrap();

        let cached_count_before: i64 = conn
            .query_row(
                "SELECT vector_count FROM kg_turboquant_cache WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cached_count_before, 3);

        // Add a 4th vector
        let e4 = crate::graph::entity::insert_entity(
            &conn,
            &crate::graph::entity::Entity::new("doc", "Doc D"),
        )
        .unwrap();
        let store = VectorStore::new();
        let mut v4 = vec![0.0f32; dim];
        v4[2] = 1.0;
        store.insert_vector(&conn, e4, v4).unwrap();

        // Second search — must rebuild and update cache to vector_count = 4
        engine.search(&conn, &embedder, "q", 2).unwrap();

        let cached_count_after: i64 = conn
            .query_row(
                "SELECT vector_count FROM kg_turboquant_cache WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            cached_count_after, 4,
            "cache should be rebuilt after new vector added"
        );
    }
}
