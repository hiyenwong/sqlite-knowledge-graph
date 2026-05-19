//! Ebbinghaus forgetting-curve confidence engine.
//!
//! Formula:
//! ```text
//! confidence(t) = base * exp(-lambda * elapsed_days)
//!              + access_bonus * ln(1 + access_count)
//!              + feedback_sum          // clamped to [-1, 1]
//! ```
//! Defaults: lambda = 0.05 / day, access_bonus = 0.1.

use crate::error::{Error, Result};
use rusqlite::Connection;
use tracing::debug;

const SECS_PER_DAY: f64 = 86_400.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Tuneable parameters for the confidence formula.
#[derive(Debug, Clone)]
pub struct ConfidenceParams {
    /// Decay rate λ (per day). Default: 0.05.
    pub lambda: f64,
    /// Reinforcement factor for access count. Default: 0.1.
    pub access_bonus: f64,
}

impl Default for ConfidenceParams {
    fn default() -> Self {
        Self {
            lambda: 0.05,
            access_bonus: 0.1,
        }
    }
}

/// Implements the Ebbinghaus forgetting-curve confidence formula.
#[derive(Default)]
pub struct ConfidenceEngine {
    pub params: ConfidenceParams,
}

impl ConfidenceEngine {
    pub fn new(params: ConfidenceParams) -> Self {
        Self { params }
    }

    /// Pure formula evaluation — no database access required.
    pub fn compute(
        &self,
        base: f64,
        lambda: f64,
        elapsed_days: f64,
        access_count: i64,
        feedback_sum: f64,
    ) -> f64 {
        let fb = feedback_sum.clamp(-1.0, 1.0);
        base * (-lambda * elapsed_days).exp()
            + self.params.access_bonus * (1.0 + access_count as f64).ln()
            + fb
    }

    /// Recompute live confidence from the entity's stored parameters and log.
    pub fn get_confidence(&self, conn: &Connection, entity_id: i64) -> Result<f64> {
        let (base, lambda, created_at, access_count) = conn
            .query_row(
                "SELECT \
                    COALESCE(base_confidence, 1.0), \
                    COALESCE(decay_rate, 0.05), \
                    COALESCE(created_at, 0), \
                    COALESCE(access_count, 0) \
                 FROM kg_entities WHERE id = ?1",
                [entity_id],
                |r| {
                    Ok((
                        r.get::<_, f64>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::EntityNotFound(entity_id),
                other => Error::SQLite(other),
            })?;

        let elapsed_days = (now_unix() - created_at).max(0) as f64 / SECS_PER_DAY;
        let feedback_sum = feedback_sum_for(conn, entity_id)?;
        let conf = self.compute(base, lambda, elapsed_days, access_count, feedback_sum);

        debug!(entity_id, elapsed_days, conf, "live confidence computed");
        Ok(conf)
    }

    /// Apply an explicit feedback adjustment, log the change, and refresh the cache.
    pub fn update_confidence(
        &self,
        conn: &Connection,
        entity_id: i64,
        feedback: f64,
    ) -> Result<f64> {
        let old_conf = self.get_confidence(conn, entity_id)?;
        let ts = now_unix();

        // Log the raw delta: new_value - old_value == feedback.
        conn.execute(
            "INSERT INTO kg_confidence_log \
             (entity_id, old_value, new_value, reason, created_at) \
             VALUES (?1, ?2, ?3, 'feedback', ?4)",
            rusqlite::params![entity_id, old_conf, old_conf + feedback, ts],
        )?;

        // Recompute with the newly inserted feedback included.
        let new_conf = self.get_confidence(conn, entity_id)?;
        conn.execute(
            "UPDATE kg_entities SET confidence = ?1 WHERE id = ?2",
            rusqlite::params![new_conf, entity_id],
        )?;

        debug!(
            entity_id,
            old_conf, new_conf, feedback, "confidence updated"
        );
        Ok(new_conf)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Sum all raw feedback deltas for an entity, clamped to [-1, 1].
fn feedback_sum_for(conn: &Connection, entity_id: i64) -> Result<f64> {
    let sum: f64 = conn.query_row(
        "SELECT COALESCE(SUM(new_value - old_value), 0.0) \
         FROM kg_confidence_log \
         WHERE entity_id = ?1 AND reason = 'feedback'",
        [entity_id],
        |r| r.get(0),
    )?;
    Ok(sum.clamp(-1.0, 1.0))
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs() as i64
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

    fn insert_entity(conn: &Connection, base: f64, lambda: f64) -> i64 {
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name, base_confidence, decay_rate) \
             VALUES ('test', 'E', ?1, ?2)",
            rusqlite::params![base, lambda],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn fresh_entity_confidence_is_base() {
        let conn = setup();
        let id = insert_entity(&conn, 1.0, 0.05);
        let engine = ConfidenceEngine::default();
        let conf = engine.get_confidence(&conn, id).unwrap();
        // elapsed ≈ 0 days, access_count=0, feedback=0 → base * exp(0) = base
        assert!((conf - 1.0).abs() < 0.01, "expected ~1.0, got {conf}");
    }

    #[test]
    fn confidence_decays_over_time() {
        let engine = ConfidenceEngine::default();
        let conf_now = engine.compute(1.0, 0.05, 0.0, 0, 0.0);
        let conf_30d = engine.compute(1.0, 0.05, 30.0, 0, 0.0);
        assert!(conf_30d < conf_now, "confidence should decay over time");
        assert!(conf_30d > 0.0, "confidence should stay positive");
    }

    #[test]
    fn access_reinforces_confidence() {
        let engine = ConfidenceEngine::default();
        let low = engine.compute(1.0, 0.05, 30.0, 0, 0.0);
        let high = engine.compute(1.0, 0.05, 30.0, 10, 0.0);
        assert!(high > low, "access should reinforce confidence");
    }

    #[test]
    fn feedback_adjusts_confidence() {
        let conn = setup();
        let id = insert_entity(&conn, 0.8, 0.0); // no decay
        let engine = ConfidenceEngine::default();

        // elapsed ≈ 0, access=0, feedback=0 → ~0.8
        let before = engine.get_confidence(&conn, id).unwrap();
        assert!((before - 0.8).abs() < 0.01, "expected ~0.8, got {before}");

        let after = engine.update_confidence(&conn, id, -0.2).unwrap();
        assert!((after - 0.6).abs() < 0.01, "expected ~0.6, got {after}");
    }

    #[test]
    fn feedback_sum_bounded() {
        let engine = ConfidenceEngine::default();
        // Very negative feedback_sum is clamped to -1.0
        let c = engine.compute(1.0, 0.0, 0.0, 0, -5.0);
        // base*1 + 0 + clamp(-5,-1,1) = 1.0 + (-1.0) = 0.0
        assert!((c - 0.0).abs() < 1e-9, "expected 0.0, got {c}");
    }

    #[test]
    fn change_logged_to_confidence_log() {
        let conn = setup();
        let id = insert_entity(&conn, 1.0, 0.0);
        let engine = ConfidenceEngine::default();
        engine.update_confidence(&conn, id, -0.1).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_confidence_log WHERE entity_id = ?1 AND reason = 'feedback'",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "feedback entry should be logged");
    }
}
