//! Database schema creation and management.

use rusqlite::Connection;

use crate::error::Result;

/// Create the knowledge graph schema in the database.
pub fn create_schema(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    // Create entities table
    tx.execute(
        r#"
        CREATE TABLE IF NOT EXISTS kg_entities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type TEXT NOT NULL,
            name TEXT NOT NULL,
            properties TEXT,  -- JSON
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            updated_at INTEGER DEFAULT (strftime('%s', 'now'))
        )
        "#,
        [],
    )?;

    // Create relations table
    tx.execute(
        r#"
        CREATE TABLE IF NOT EXISTS kg_relations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id INTEGER NOT NULL,
            target_id INTEGER NOT NULL,
            rel_type TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            properties TEXT,  -- JSON
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            FOREIGN KEY (source_id) REFERENCES kg_entities(id) ON DELETE CASCADE,
            FOREIGN KEY (target_id) REFERENCES kg_entities(id) ON DELETE CASCADE
        )
        "#,
        [],
    )?;

    // Create vectors table
    tx.execute(
        r#"
        CREATE TABLE IF NOT EXISTS kg_vectors (
            entity_id INTEGER NOT NULL PRIMARY KEY,
            vector BLOB NOT NULL,
            dimension INTEGER NOT NULL,
            created_at INTEGER DEFAULT (strftime('%s', 'now')),
            FOREIGN KEY (entity_id) REFERENCES kg_entities(id) ON DELETE CASCADE
        )
        "#,
        [],
    )?;

    // Create indexes
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_type ON kg_entities(entity_type)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_entities_name ON kg_entities(name)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_relations_source ON kg_relations(source_id)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_relations_target ON kg_relations(target_id)",
        [],
    )?;

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_relations_type ON kg_relations(rel_type)",
        [],
    )?;

    tx.commit()?;
    Ok(())
}

/// Check if the schema exists.
pub fn schema_exists(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kg_entities'")?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count > 0)
}
