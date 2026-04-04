//! Database schema creation and management.
//!
//! # Versioned migration system
//!
//! The schema is versioned via the `kg_schema_version` table (single-row).
//! Call [`ensure_schema`] on every database open to apply any pending migrations
//! automatically.
//!
//! | Version | Changes |
//! |---------|---------|
//! | 1       | Initial schema: entities, relations, vectors, hyperedges, turboquant cache |
//! | 2       | Add `vectors_checksum` column to `kg_turboquant_cache` |

use rusqlite::Connection;

use crate::error::{Error, Result};

/// Latest known schema version.  Bump this whenever a new migration is added.
const CURRENT_SCHEMA_VERSION: i32 = 2;

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Ensure the database schema is up to date, running any pending migrations.
///
/// Safe to call on:
/// - A brand-new empty database (applies all migrations from scratch).
/// - An existing database without version tracking (detects v1 tables, starts
///   from v1 so existing data is preserved).
/// - An already fully-migrated database (fast no-op path).
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    // Bootstrap: create the version table.  This is idempotent.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kg_schema_version (version INTEGER NOT NULL);",
    )?;

    // Read the stored version, if any.
    let stored: Option<i32> = conn
        .query_row("SELECT version FROM kg_schema_version", [], |r| r.get(0))
        .ok();

    let current_version = match stored {
        Some(v) => v,
        None => {
            // No version row yet.  Distinguish a legacy DB (core tables already
            // exist) from a fresh one so we never re-create existing tables.
            if schema_exists(conn)? {
                1 // Legacy database: all v1 tables are present, start from there.
            } else {
                0 // Brand-new database: nothing applied yet.
            }
        }
    };

    if current_version >= CURRENT_SCHEMA_VERSION {
        return Ok(()); // Already up to date — fast path.
    }

    // Apply all pending migrations inside a single transaction.
    let tx = conn.unchecked_transaction()?;
    for v in (current_version + 1)..=CURRENT_SCHEMA_VERSION {
        apply_migration(&tx, v)?;
    }

    // Persist the new version (replace any existing row).
    tx.execute("DELETE FROM kg_schema_version", [])?;
    tx.execute(
        "INSERT INTO kg_schema_version (version) VALUES (?1)",
        [CURRENT_SCHEMA_VERSION],
    )?;

    tx.commit()?;
    Ok(())
}

/// Create the knowledge graph schema in the database.
///
/// Alias for [`ensure_schema`] kept for backward compatibility.
#[inline]
pub fn create_schema(conn: &Connection) -> Result<()> {
    ensure_schema(conn)
}

/// Return the current schema version stored in the database.
///
/// Returns `None` if `kg_schema_version` has not been populated yet (e.g. a
/// legacy DB that has never been opened with this library version).
pub fn schema_version(conn: &Connection) -> Result<Option<i32>> {
    let v = conn
        .query_row("SELECT version FROM kg_schema_version", [], |r| r.get(0))
        .ok();
    Ok(v)
}

/// Check if the core schema tables exist (used for legacy-DB detection).
pub fn schema_exists(conn: &Connection) -> Result<bool> {
    let mut stmt = conn
        .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kg_entities'")?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Migration runner
// ─────────────────────────────────────────────────────────────────────────────

fn apply_migration(conn: &Connection, version: i32) -> Result<()> {
    match version {
        1 => migration_v1(conn),
        2 => migration_v2(conn),
        _ => Err(Error::Other(format!(
            "Unknown schema migration version: {}",
            version
        ))),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Migrations
// ─────────────────────────────────────────────────────────────────────────────

/// Migration v1 — initial schema (all core tables and indexes).
fn migration_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS kg_entities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type TEXT NOT NULL,
            name TEXT NOT NULL,
            properties TEXT,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            updated_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS kg_relations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id INTEGER NOT NULL,
            target_id INTEGER NOT NULL,
            rel_type TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            properties TEXT,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            FOREIGN KEY (source_id) REFERENCES kg_entities(id) ON DELETE CASCADE,
            FOREIGN KEY (target_id) REFERENCES kg_entities(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS kg_vectors (
            entity_id INTEGER NOT NULL PRIMARY KEY,
            vector BLOB NOT NULL,
            dimension INTEGER NOT NULL,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            FOREIGN KEY (entity_id) REFERENCES kg_entities(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_entities_type ON kg_entities(entity_type);
        CREATE INDEX IF NOT EXISTS idx_entities_name ON kg_entities(name);
        CREATE INDEX IF NOT EXISTS idx_relations_source ON kg_relations(source_id);
        CREATE INDEX IF NOT EXISTS idx_relations_target ON kg_relations(target_id);
        CREATE INDEX IF NOT EXISTS idx_relations_type ON kg_relations(rel_type);

        CREATE TABLE IF NOT EXISTS kg_hyperedges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            hyperedge_type TEXT NOT NULL,
            entity_ids TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            arity INTEGER NOT NULL,
            properties TEXT,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            updated_at INTEGER DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS kg_hyperedge_entities (
            hyperedge_id INTEGER NOT NULL,
            entity_id INTEGER NOT NULL,
            position INTEGER NOT NULL,
            PRIMARY KEY (hyperedge_id, entity_id),
            FOREIGN KEY (hyperedge_id) REFERENCES kg_hyperedges(id) ON DELETE CASCADE,
            FOREIGN KEY (entity_id) REFERENCES kg_entities(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_hyperedges_type ON kg_hyperedges(hyperedge_type);
        CREATE INDEX IF NOT EXISTS idx_hyperedges_arity ON kg_hyperedges(arity);
        CREATE INDEX IF NOT EXISTS idx_he_entities_entity ON kg_hyperedge_entities(entity_id);
        CREATE INDEX IF NOT EXISTS idx_he_entities_hyperedge ON kg_hyperedge_entities(hyperedge_id);

        CREATE TABLE IF NOT EXISTS kg_turboquant_cache (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            index_blob BLOB NOT NULL,
            vector_count INTEGER NOT NULL
        );
        "#,
    )?;
    Ok(())
}

/// Migration v2 — add `vectors_checksum` to `kg_turboquant_cache`.
///
/// The checksum (`COALESCE(SUM(entity_id), 0)` over `kg_vectors`) is a
/// lightweight fingerprint that detects cache staleness even when the
/// *count* of vectors stays the same — for example when one vector is
/// deleted and a different one is inserted.
fn migration_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE kg_turboquant_cache \
         ADD COLUMN vectors_checksum INTEGER NOT NULL DEFAULT 0;",
    )?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_fresh_db_reaches_current_version() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        let v = schema_version(&conn).unwrap();
        assert_eq!(v, Some(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn test_idempotent_second_call() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        // Should not error and version stays the same.
        ensure_schema(&conn).unwrap();
        let v = schema_version(&conn).unwrap();
        assert_eq!(v, Some(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn test_legacy_db_migrates_from_v1() {
        let conn = Connection::open_in_memory().unwrap();

        // Simulate a v1 database: apply only migration_v1 manually (no version row).
        migration_v1(&conn).unwrap();
        assert!(schema_exists(&conn).unwrap());
        assert_eq!(schema_version(&conn).unwrap(), None); // no version yet

        // Now run ensure_schema: should detect legacy v1 and apply only v2.
        ensure_schema(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), Some(CURRENT_SCHEMA_VERSION));

        // The vectors_checksum column must now exist.
        conn.execute(
            "INSERT INTO kg_turboquant_cache (id, index_blob, vector_count, vectors_checksum) \
             VALUES (1, X'', 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn test_all_tables_created() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();

        let tables = [
            "kg_entities",
            "kg_relations",
            "kg_vectors",
            "kg_hyperedges",
            "kg_hyperedge_entities",
            "kg_turboquant_cache",
            "kg_schema_version",
        ];

        for table in &tables {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} should exist");
        }
    }

    #[test]
    fn test_create_schema_alias() {
        // create_schema must behave identically to ensure_schema.
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), Some(CURRENT_SCHEMA_VERSION));
    }
}
