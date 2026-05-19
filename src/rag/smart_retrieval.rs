//! Four-signal retrieval scoring for SmartVector.
//!
//! Final score = w1·cosine + w2·temporal + w3·confidence + w4·graph_importance
//!
//! Default weights: w1=0.5, w2=0.2, w3=0.2, w4=0.1.

use crate::error::{Error, Result};
use crate::graph::get_entity;
use crate::vector::confidence::now_unix;
use crate::vector::VectorStore;
use rusqlite::Connection;
use std::collections::HashMap;
use tracing::debug;

const TEMPORAL_DECAY_FACTOR: f64 = 0.1; // per-day decay outside validity window
const SECS_PER_DAY: f64 = 86_400.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Blending weights for the four retrieval signals.
#[derive(Debug, Clone, Copy)]
pub struct RetrievalWeights {
    pub w1: f64, // cosine similarity
    pub w2: f64, // temporal validity
    pub w3: f64, // live confidence
    pub w4: f64, // graph importance
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            w1: 0.5,
            w2: 0.2,
            w3: 0.2,
            w4: 0.1,
        }
    }
}

/// One result from four-signal retrieval.
#[derive(Debug, Clone)]
pub struct SmartSearchResult {
    pub entity: crate::graph::Entity,
    pub final_score: f64,
    pub cosine_score: f64,
    pub temporal_score: f64,
    pub confidence_score: f64,
    pub graph_importance: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// SmartRetrieval
// ─────────────────────────────────────────────────────────────────────────────

/// Retrieval engine that combines four signals.
#[derive(Default)]
pub struct SmartRetrieval {
    pub weights: RetrievalWeights,
}

impl SmartRetrieval {
    pub fn new(weights: RetrievalWeights) -> Self {
        Self { weights }
    }

    pub fn set_weights(&mut self, weights: RetrievalWeights) {
        self.weights = weights;
    }

    /// Retrieve top-`k` entities scored by the four-signal formula.
    pub fn retrieve(
        &self,
        conn: &Connection,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SmartSearchResult>> {
        let store = VectorStore::new();
        // Fetch a larger candidate pool so re-ranking has enough options.
        let pool_size = (top_k * 3).max(20);
        let candidates = store.search_vectors(conn, query.to_vec(), pool_size)?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let ids: Vec<i64> = candidates.iter().map(|c| c.entity_id).collect();
        let indegrees = load_indegrees(conn, &ids)?;
        let max_indegree = indegrees.values().copied().fold(0u32, u32::max);
        let now = now_unix();

        let mut results = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            let eid = candidate.entity_id;
            let cosine = candidate.similarity as f64;
            let temporal = temporal_validity(conn, eid, now)?;
            let conf = cached_confidence(conn, eid)?;
            let importance = if max_indegree > 0 {
                *indegrees.get(&eid).unwrap_or(&0) as f64 / max_indegree as f64
            } else {
                0.0
            };

            let final_score = self.weights.w1 * cosine
                + self.weights.w2 * temporal
                + self.weights.w3 * conf
                + self.weights.w4 * importance;

            let entity = get_entity(conn, eid)?;
            results.push(SmartSearchResult {
                entity,
                final_score,
                cosine_score: cosine,
                temporal_score: temporal,
                confidence_score: conf,
                graph_importance: importance,
            });
        }

        results.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        debug!(
            top_k,
            found = results.len(),
            "four-signal retrieval complete"
        );
        Ok(results)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn load_indegrees(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, u32>> {
    let mut map = HashMap::with_capacity(ids.len());
    for &id in ids {
        let count: u32 = conn.query_row(
            "SELECT COUNT(*) FROM kg_dependencies WHERE target_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        map.insert(id, count);
    }
    Ok(map)
}

/// Returns a score in [0, 1] reflecting how valid the entity is right now.
fn temporal_validity(conn: &Connection, entity_id: i64, now: i64) -> Result<f64> {
    let (valid_from, valid_until): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT valid_from, valid_until FROM kg_entities WHERE id = ?1",
            [entity_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::EntityNotFound(entity_id),
            other => Error::SQLite(other),
        })?;

    if let Some(from) = valid_from {
        if now < from {
            return Ok(0.0); // not yet valid
        }
    }
    if let Some(until) = valid_until {
        if now > until {
            let days_over = (now - until) as f64 / SECS_PER_DAY;
            return Ok((-TEMPORAL_DECAY_FACTOR * days_over).exp());
        }
    }
    Ok(1.0)
}

fn cached_confidence(conn: &Connection, entity_id: i64) -> Result<f64> {
    conn.query_row(
        "SELECT COALESCE(confidence, 1.0) FROM kg_entities WHERE id = ?1",
        [entity_id],
        |r| r.get(0),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::EntityNotFound(entity_id),
        other => Error::SQLite(other),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ensure_schema;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        conn
    }

    fn add_entity_with_vector(conn: &Connection, name: &str, vec: &[f32]) -> i64 {
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name) VALUES ('t', ?1)",
            [name],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        let store = VectorStore::new();
        store.insert_vector(conn, id, vec.to_vec()).unwrap();
        id
    }

    #[test]
    fn retrieves_top_k_results() {
        let conn = setup();
        add_entity_with_vector(&conn, "A", &[1.0, 0.0, 0.0]);
        add_entity_with_vector(&conn, "B", &[0.9, 0.1, 0.0]);
        add_entity_with_vector(&conn, "C", &[0.0, 0.0, 1.0]);

        let sr = SmartRetrieval::default();
        let results = sr.retrieve(&conn, &[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        // A or B should be at the top (most similar to query)
        assert!(results[0].cosine_score >= results[1].cosine_score - 0.1);
    }

    #[test]
    fn temporal_past_window_decays_score() {
        let conn = setup();
        let id = add_entity_with_vector(&conn, "old", &[1.0, 0.0]);
        // Set valid_until 365 days in the past
        let past = now_unix() - 365 * 86400;
        conn.execute(
            "UPDATE kg_entities SET valid_until = ?1 WHERE id = ?2",
            rusqlite::params![past, id],
        )
        .unwrap();

        let score = temporal_validity(&conn, id, now_unix()).unwrap();
        assert!(
            score < 0.01,
            "expired entity should have near-zero temporal score"
        );
    }

    #[test]
    fn temporal_future_window_returns_zero() {
        let conn = setup();
        let id = add_entity_with_vector(&conn, "future", &[1.0, 0.0]);
        let future = now_unix() + 86400;
        conn.execute(
            "UPDATE kg_entities SET valid_from = ?1 WHERE id = ?2",
            rusqlite::params![future, id],
        )
        .unwrap();

        let score = temporal_validity(&conn, id, now_unix()).unwrap();
        assert_eq!(
            score, 0.0,
            "not-yet-valid entity should have zero temporal score"
        );
    }

    #[test]
    fn configurable_weights_affect_ranking() {
        let conn = setup();
        let _id_a = add_entity_with_vector(&conn, "A", &[1.0, 0.0]);
        let id_b = add_entity_with_vector(&conn, "B", &[0.5, 0.5]);

        // Give B higher confidence
        conn.execute(
            "UPDATE kg_entities SET confidence = 2.0 WHERE id = ?1",
            [id_b],
        )
        .unwrap();

        // High confidence weight: B might rank above A despite lower cosine
        let mut sr = SmartRetrieval::default();
        sr.set_weights(RetrievalWeights {
            w1: 0.1,
            w2: 0.1,
            w3: 0.7,
            w4: 0.1,
        });
        let results = sr.retrieve(&conn, &[1.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        // B should now rank first due to high confidence weight
        assert_eq!(results[0].entity.id, Some(id_b));
    }

    #[test]
    fn graph_importance_boosts_score() {
        let conn = setup();
        let _id_a = add_entity_with_vector(&conn, "A", &[1.0, 0.0]);
        let id_b = add_entity_with_vector(&conn, "B", &[1.0, 0.0]);

        // Make several entities depend on B (high in-degree)
        for _ in 0..5 {
            conn.execute(
                "INSERT INTO kg_entities (entity_type, name) VALUES ('dep', 'dep')",
                [],
            )
            .unwrap();
            let dep_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO kg_dependencies (source_id, target_id, dep_type) VALUES (?1, ?2, 'depends_on')",
                rusqlite::params![dep_id, id_b],
            )
            .unwrap();
        }

        let sr = SmartRetrieval::new(RetrievalWeights {
            w1: 0.0,
            w2: 0.0,
            w3: 0.0,
            w4: 1.0, // only graph importance
        });
        let results = sr.retrieve(&conn, &[1.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        // B has higher in-degree so should rank first
        assert_eq!(
            results[0].entity.id,
            Some(id_b),
            "high in-degree entity should rank first"
        );
        assert!(
            results[0].graph_importance > results[1].graph_importance,
            "importance should be normalised"
        );
    }
}
