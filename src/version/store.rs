//! Version CRUD operations.

use rusqlite::{params, OptionalExtension};

use super::{bit_from_slot, Version, MAX_VERSIONS};
use crate::error::{Error, Result};

/// Create a new version. Returns the version ID.
///
/// Allocates the lowest free `bit_slot` in `[0, 63]`.  Returns
/// [`Error::VersionLimitExceeded`] when all 64 slots are occupied by live
/// versions (deleting a version frees its slot for reuse).
pub fn create_version(
    conn: &rusqlite::Connection,
    name: &str,
    branch: &str,
    parent_id: Option<i64>,
    description: Option<&str>,
) -> Result<i64> {
    let slot = allocate_slot(conn)?;
    conn.execute(
        "INSERT INTO kg_versions (name, branch, parent_id, description, bit_slot) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, branch, parent_id, description, slot],
    )
    .map_err(|e| {
        if e.to_string()
            .contains("UNIQUE constraint failed: kg_versions.name")
        {
            Error::DuplicateVersionName(name.to_string())
        } else {
            Error::from(e)
        }
    })?;
    Ok(conn.last_insert_rowid())
}

/// Find the lowest unused `bit_slot` in `[0, 63]`.
fn allocate_slot(conn: &rusqlite::Connection) -> Result<i64> {
    let mut stmt = conn.prepare("SELECT bit_slot FROM kg_versions")?;
    let used: std::collections::HashSet<i64> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    (0..MAX_VERSIONS)
        .find(|slot| !used.contains(slot))
        .ok_or(Error::VersionLimitExceeded)
}

/// Delete a version by ID, clearing its bit from every entity and relation so
/// the freed slot can be safely reused by a future version.  Runs in a single
/// transaction: either the version row and all its bits go, or nothing does.
pub fn delete_version(conn: &rusqlite::Connection, version_id: i64) -> Result<()> {
    let bit = version_bit_for(conn, version_id)?; // also validates existence

    let tx = conn.unchecked_transaction()?;
    clear_bit(&tx, "kg_entities", bit)?;
    clear_bit(&tx, "kg_relations", bit)?;
    tx.execute("DELETE FROM kg_versions WHERE id = ?1", [version_id])?;
    tx.commit()?;
    Ok(())
}

/// Clear `bit` from the validity column of every row in `table`, collapsing a
/// resulting 0 back to NULL (the unversioned sentinel).
fn clear_bit(conn: &rusqlite::Connection, table: &str, bit: i64) -> Result<()> {
    // `table` is a hard-coded literal at every call site, never user input.
    conn.execute(
        &format!(
            "UPDATE {table} SET validity = CASE \
             WHEN (validity & ~?1) = 0 THEN NULL ELSE validity & ~?1 END \
             WHERE validity IS NOT NULL AND (validity & ?1) != 0"
        ),
        [bit],
    )?;
    Ok(())
}

/// Resolve a version id to its validity bitmask (`1 << bit_slot`).
/// Returns [`Error::VersionNotFound`] if the version does not exist.
pub fn version_bit_for(conn: &rusqlite::Connection, version_id: i64) -> Result<i64> {
    let slot: i64 = conn
        .query_row(
            "SELECT bit_slot FROM kg_versions WHERE id = ?1",
            [version_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(Error::VersionNotFound(version_id))?;
    Ok(bit_from_slot(slot))
}

/// Return every version whose `bit_slot` is set in `bits`, newest first.
pub fn versions_for_bits(conn: &rusqlite::Connection, bits: i64) -> Result<Vec<Version>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, branch, parent_id, description, created_at, is_merged \
         FROM kg_versions WHERE (?1 & (1 << bit_slot)) != 0 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([bits], row_to_version)?;
    let mut versions = Vec::new();
    for row in rows {
        versions.push(row?);
    }
    Ok(versions)
}

/// List versions, optionally filtered by branch.
pub fn list_versions(conn: &rusqlite::Connection, branch: Option<&str>) -> Result<Vec<Version>> {
    let query = if branch.is_some() {
        "SELECT id, name, branch, parent_id, description, created_at, is_merged \
         FROM kg_versions WHERE branch = ?1 ORDER BY created_at DESC"
    } else {
        "SELECT id, name, branch, parent_id, description, created_at, is_merged \
         FROM kg_versions ORDER BY created_at DESC"
    };

    let mut stmt = conn.prepare(query)?;

    let rows = if let Some(b) = branch {
        stmt.query_map(params![b], row_to_version)?
    } else {
        stmt.query_map([], row_to_version)?
    };

    let mut versions = Vec::new();
    for row in rows {
        versions.push(row?);
    }
    Ok(versions)
}

/// Get a version by ID. Returns None if not found.
pub fn get_version(conn: &rusqlite::Connection, version_id: i64) -> Result<Version> {
    conn.query_row(
        "SELECT id, name, branch, parent_id, description, created_at, is_merged \
         FROM kg_versions WHERE id = ?1",
        [version_id],
        row_to_version,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::VersionNotFound(version_id),
        other => Error::from(other),
    })
}

/// Check that a version exists. Returns error if not found.
pub fn ensure_version_exists(conn: &rusqlite::Connection, version_id: i64) -> Result<()> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM kg_versions WHERE id = ?1",
            [version_id],
            |r| r.get(0),
        )
        .map_err(Error::from)?;
    if !exists {
        return Err(Error::VersionNotFound(version_id));
    }
    Ok(())
}

/// Check that an entity exists. Returns error if not found.
pub fn ensure_entity_exists(conn: &rusqlite::Connection, entity_id: i64) -> Result<()> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM kg_entities WHERE id = ?1",
            [entity_id],
            |r| r.get(0),
        )
        .map_err(Error::from)?;
    if !exists {
        return Err(Error::EntityNotFound(entity_id));
    }
    Ok(())
}

/// Check that a relation exists. Returns error if not found.
pub fn ensure_relation_exists(conn: &rusqlite::Connection, relation_id: i64) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM kg_relations WHERE id = ?1",
        [relation_id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(Error::RelationNotFound(relation_id));
    }
    Ok(())
}

fn row_to_version(row: &rusqlite::Row) -> rusqlite::Result<Version> {
    Ok(Version {
        id: row.get(0)?,
        name: row.get(1)?,
        branch: row.get(2)?,
        parent_id: row.get(3)?,
        description: row.get(4)?,
        created_at: row.get(5)?,
        is_merged: row.get::<_, i64>(6)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_create_version() {
        let conn = setup();
        let id = create_version(&conn, "v1", "main", None, Some("first")).unwrap();
        assert!(id > 0);

        let v = get_version(&conn, id).unwrap();
        assert_eq!(v.name, "v1");
        assert_eq!(v.branch, "main");
        assert_eq!(v.description.as_deref(), Some("first"));
        assert!(!v.is_merged);
    }

    #[test]
    fn test_duplicate_name_rejected() {
        let conn = setup();
        create_version(&conn, "v1", "main", None, None).unwrap();
        let err = create_version(&conn, "v1", "main", None, None).unwrap_err();
        assert!(matches!(err, Error::DuplicateVersionName(_)));
    }

    #[test]
    fn test_delete_version() {
        let conn = setup();
        let id = create_version(&conn, "v1", "main", None, None).unwrap();
        delete_version(&conn, id).unwrap();
        assert!(get_version(&conn, id).is_err());
    }

    #[test]
    fn test_delete_nonexistent() {
        let conn = setup();
        let err = delete_version(&conn, 999).unwrap_err();
        assert!(matches!(err, Error::VersionNotFound(999)));
    }

    #[test]
    fn test_list_all_versions() {
        let conn = setup();
        create_version(&conn, "v1", "main", None, None).unwrap();
        create_version(&conn, "v2", "main", None, None).unwrap();
        create_version(&conn, "v1-feat", "feature", None, None).unwrap();

        let all = list_versions(&conn, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_list_by_branch() {
        let conn = setup();
        create_version(&conn, "v1", "main", None, None).unwrap();
        create_version(&conn, "v2", "main", None, None).unwrap();
        create_version(&conn, "v1-feat", "feature", None, None).unwrap();

        let main = list_versions(&conn, Some("main")).unwrap();
        assert_eq!(main.len(), 2);
        assert!(main.iter().all(|v| v.branch == "main"));
    }

    fn add_entity(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name) VALUES ('t', 'X')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn validity(conn: &Connection, eid: i64) -> Option<i64> {
        conn.query_row(
            "SELECT validity FROM kg_entities WHERE id = ?1",
            [eid],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn test_first_version_uses_slot_zero() {
        let conn = setup();
        let id = create_version(&conn, "v1", "main", None, None).unwrap();
        assert_eq!(version_bit_for(&conn, id).unwrap(), 1); // 1 << 0
    }

    #[test]
    fn test_delete_clears_bits_and_collapses_to_null() {
        let conn = setup();
        let eid = add_entity(&conn);
        let v1 = create_version(&conn, "v1", "main", None, None).unwrap();
        crate::version::snapshot::version_add_entity(&conn, v1, eid).unwrap();
        assert_eq!(validity(&conn, eid), Some(1));

        delete_version(&conn, v1).unwrap();
        // The only version is gone, so the entity returns to unversioned (NULL).
        assert_eq!(validity(&conn, eid), None);
    }

    #[test]
    fn test_slot_reclaimed_without_leaking_stale_bits() {
        let conn = setup();
        let eid = add_entity(&conn);
        let v1 = create_version(&conn, "v1", "main", None, None).unwrap();
        crate::version::snapshot::version_add_entity(&conn, v1, eid).unwrap();

        // Deleting v1 frees slot 0; the next version must reuse it cleanly.
        delete_version(&conn, v1).unwrap();
        let v2 = create_version(&conn, "v2", "main", None, None).unwrap();
        assert_eq!(version_bit_for(&conn, v2).unwrap(), 1); // slot 0 reused

        // The entity must NOT have leaked into v2 via the recycled bit.
        let in_v2 = crate::version::query::version_entities(&conn, v2, None, None).unwrap();
        assert!(in_v2.is_empty());
    }

    #[test]
    fn test_version_limit_exceeded() {
        let conn = setup();
        for i in 0..64 {
            create_version(&conn, &format!("v{i}"), "main", None, None).unwrap();
        }
        let err = create_version(&conn, "v64", "main", None, None).unwrap_err();
        assert!(matches!(err, Error::VersionLimitExceeded));
    }

    #[test]
    fn test_version_bit_for_unknown_errors() {
        let conn = setup();
        let err = version_bit_for(&conn, 999).unwrap_err();
        assert!(matches!(err, Error::VersionNotFound(999)));
    }
}
