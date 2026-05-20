//! Ripple propagation of confidence penalties along dependency edges.
//!
//! When an entity becomes stale or its confidence drops, dependent entities
//! receive an attenuated penalty via BFS up to `MAX_DEPTH = 2` hops.
//! Every change is appended to `kg_confidence_log`.

use crate::error::Result;
use crate::vector::confidence::now_unix;
use rusqlite::Connection;
use std::collections::{HashSet, VecDeque};
use tracing::debug;

const MAX_DEPTH: usize = 2;
const ATTENUATION: f64 = 0.5;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Propagate a confidence penalty from `origin_id` to all entities that
/// transitively depend on it, up to `MAX_DEPTH` hops.
///
/// Penalty at hop *h* = `base_penalty * ATTENUATION^h`.
pub fn propagate(conn: &Connection, origin_id: i64, base_penalty: f64) -> Result<()> {
    // BFS queue: (entity_id, hop_depth)
    let mut queue: VecDeque<(i64, usize)> = VecDeque::new();
    let mut visited: HashSet<i64> = HashSet::new();
    visited.insert(origin_id);
    queue.push_back((origin_id, 0));

    while let Some((current_id, depth)) = queue.pop_front() {
        if depth >= MAX_DEPTH {
            continue;
        }

        let next_depth = depth + 1;
        let actual_penalty = base_penalty * ATTENUATION.powi(next_depth as i32);

        // Find entities whose confidence depends_on current_id
        let dependents = dependents_of(conn, current_id)?;

        for dep_id in dependents {
            if visited.contains(&dep_id) {
                continue;
            }
            visited.insert(dep_id);

            apply_penalty(conn, dep_id, actual_penalty)?;
            debug!(
                dep_id,
                depth = next_depth,
                actual_penalty,
                "ripple penalty applied"
            );
            queue.push_back((dep_id, next_depth));
        }
    }

    Ok(())
}

/// Insert a dependency edge: `source_id` depends on `target_id`.
///
/// `dep_type` should be one of: `depends_on`, `depended_by`, `supersedes`,
/// `contradicts`.
pub fn add_dependency(
    conn: &Connection,
    source_id: i64,
    target_id: i64,
    dep_type: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO kg_dependencies (source_id, target_id, dep_type, created_at) \
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![source_id, target_id, dep_type, now_unix()],
    )?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn dependents_of(conn: &Connection, target_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT source_id FROM kg_dependencies \
         WHERE target_id = ?1 AND dep_type = 'depends_on'",
    )?;
    let ids = stmt
        .query_map([target_id], |r| r.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ids)
}

fn apply_penalty(conn: &Connection, entity_id: i64, penalty: f64) -> Result<()> {
    let old_conf: f64 = conn.query_row(
        "SELECT COALESCE(confidence, 1.0) FROM kg_entities WHERE id = ?1",
        [entity_id],
        |r| r.get(0),
    )?;
    let raw_conf = old_conf - penalty;
    let new_conf = raw_conf.clamp(0.0, 1.0);

    conn.execute(
        "UPDATE kg_entities SET confidence = ?1 WHERE id = ?2",
        rusqlite::params![new_conf, entity_id],
    )?;
    conn.execute(
        "INSERT INTO kg_confidence_log \
         (entity_id, old_value, new_value, reason, created_at) \
         VALUES (?1, ?2, ?3, 'ripple', ?4)",
        rusqlite::params![entity_id, old_conf, new_conf, now_unix()],
    )?;
    Ok(())
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

    fn insert_entity(conn: &Connection, name: &str) -> i64 {
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name, confidence) VALUES ('t', ?1, 1.0)",
            [name],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn get_confidence(conn: &Connection, id: i64) -> f64 {
        conn.query_row(
            "SELECT confidence FROM kg_entities WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn add_dependency_inserts_row() {
        let conn = setup();
        let a = insert_entity(&conn, "A");
        let b = insert_entity(&conn, "B");
        add_dependency(&conn, a, b, "depends_on").unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM kg_dependencies", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ripple_penalises_direct_dependent() {
        let conn = setup();
        let b = insert_entity(&conn, "B");
        let a = insert_entity(&conn, "A");
        add_dependency(&conn, a, b, "depends_on").unwrap(); // A depends on B

        propagate(&conn, b, 0.4).unwrap();

        // A should have been penalised at depth 1: penalty = 0.4 * 0.5 = 0.2
        let conf_a = get_confidence(&conn, a);
        assert!((conf_a - 0.8).abs() < 1e-9, "expected 0.8, got {conf_a}");
    }

    #[test]
    fn ripple_attenuates_at_depth_two() {
        let conn = setup();
        let c_node = insert_entity(&conn, "C"); // C depends on B
        let b = insert_entity(&conn, "B"); // B depends on A
        let a = insert_entity(&conn, "A"); // origin: A becomes stale

        add_dependency(&conn, b, a, "depends_on").unwrap();
        add_dependency(&conn, c_node, b, "depends_on").unwrap();

        propagate(&conn, a, 0.4).unwrap();

        let conf_b = get_confidence(&conn, b);
        let conf_c = get_confidence(&conn, c_node);

        // depth 1: 0.4 * 0.5 = 0.2 → conf_b = 0.8
        assert!((conf_b - 0.8).abs() < 1e-9, "expected 0.8, got {conf_b}");
        // depth 2: 0.4 * 0.25 = 0.1 → conf_c = 0.9
        assert!((conf_c - 0.9).abs() < 1e-9, "expected 0.9, got {conf_c}");
    }

    #[test]
    fn ripple_logs_changes() {
        let conn = setup();
        let b = insert_entity(&conn, "B");
        let a = insert_entity(&conn, "A");
        add_dependency(&conn, a, b, "depends_on").unwrap();

        propagate(&conn, b, 0.2).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_confidence_log WHERE reason = 'ripple'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ripple_stops_at_max_depth() {
        let conn = setup();
        // Chain: D → C → B → A  (A is origin)
        let d = insert_entity(&conn, "D");
        let c_node = insert_entity(&conn, "C");
        let b = insert_entity(&conn, "B");
        let a = insert_entity(&conn, "A");

        add_dependency(&conn, b, a, "depends_on").unwrap();
        add_dependency(&conn, c_node, b, "depends_on").unwrap();
        add_dependency(&conn, d, c_node, "depends_on").unwrap();

        propagate(&conn, a, 0.4).unwrap();

        // D is at depth 3 — should NOT be penalised
        let conf_d = get_confidence(&conn, d);
        assert!((conf_d - 1.0).abs() < 1e-9, "D should be unaffected");
    }

    #[test]
    fn apply_penalty_clamps_to_zero_when_penalty_exceeds_confidence() {
        let conn = setup();
        let a = insert_entity(&conn, "A");

        // penalty larger than the starting confidence of 1.0
        apply_penalty(&conn, a, 1.5).unwrap();

        let conf = get_confidence(&conn, a);
        assert!(conf >= 0.0, "confidence must not be negative, got {conf}");
        assert!((conf - 0.0).abs() < 1e-9, "expected 0.0, got {conf}");
    }
}
