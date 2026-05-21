//! Version snapshot operations — add/remove entities and relations to/from versions.

use rusqlite::params;

use super::store;
use crate::error::Result;

/// Add an entity to a version (set the bit in its validity bitstring).
pub fn version_add_entity(
    conn: &rusqlite::Connection,
    version_id: i64,
    entity_id: i64,
) -> Result<()> {
    let bit = store::version_bit_for(conn, version_id)?;
    store::ensure_entity_exists(conn, entity_id)?;

    conn.execute(
        "UPDATE kg_entities SET validity = COALESCE(validity, 0) | ?1 WHERE id = ?2",
        params![bit, entity_id],
    )?;
    Ok(())
}

/// Remove an entity from a version (clear the bit). If result is 0, set to NULL.
pub fn version_remove_entity(
    conn: &rusqlite::Connection,
    version_id: i64,
    entity_id: i64,
) -> Result<()> {
    let bit = store::version_bit_for(conn, version_id)?;
    store::ensure_entity_exists(conn, entity_id)?;

    // Clear the bit, then check if result is 0 → set to NULL
    conn.execute(
        "UPDATE kg_entities SET validity = CASE \
         WHEN (COALESCE(validity, 0) & ~?1) = 0 THEN NULL \
         ELSE validity & ~?1 \
         END \
         WHERE id = ?2",
        params![bit, entity_id],
    )?;
    Ok(())
}

/// Add a relation to a version.
pub fn version_add_relation(
    conn: &rusqlite::Connection,
    version_id: i64,
    relation_id: i64,
) -> Result<()> {
    let bit = store::version_bit_for(conn, version_id)?;
    store::ensure_relation_exists(conn, relation_id)?;

    conn.execute(
        "UPDATE kg_relations SET validity = COALESCE(validity, 0) | ?1 WHERE id = ?2",
        params![bit, relation_id],
    )?;
    Ok(())
}

/// Remove a relation from a version. If result is 0, set to NULL.
pub fn version_remove_relation(
    conn: &rusqlite::Connection,
    version_id: i64,
    relation_id: i64,
) -> Result<()> {
    let bit = store::version_bit_for(conn, version_id)?;
    store::ensure_relation_exists(conn, relation_id)?;

    conn.execute(
        "UPDATE kg_relations SET validity = CASE \
         WHEN (COALESCE(validity, 0) & ~?1) = 0 THEN NULL \
         ELSE validity & ~?1 \
         END \
         WHERE id = ?2",
        params![bit, relation_id],
    )?;
    Ok(())
}

/// Bulk add all entities to a version in a single operation.
pub fn version_snapshot_entities(conn: &rusqlite::Connection, version_id: i64) -> Result<()> {
    let bit = store::version_bit_for(conn, version_id)?;
    conn.execute(
        "UPDATE kg_entities SET validity = COALESCE(validity, 0) | ?1",
        [bit],
    )?;
    Ok(())
}

/// Bulk add all relations to a version in a single operation.
pub fn version_snapshot_relations(conn: &rusqlite::Connection, version_id: i64) -> Result<()> {
    let bit = store::version_bit_for(conn, version_id)?;
    conn.execute(
        "UPDATE kg_relations SET validity = COALESCE(validity, 0) | ?1",
        [bit],
    )?;
    Ok(())
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

    fn insert_entity(conn: &Connection, name: &str) -> i64 {
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name) VALUES ('test', ?1)",
            [name],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_relation(conn: &Connection, src: i64, tgt: i64) -> i64 {
        conn.execute(
            "INSERT INTO kg_relations (source_id, target_id, rel_type) VALUES (?1, ?2, 'rel')",
            rusqlite::params![src, tgt],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn get_validity(conn: &Connection, table: &str, id: i64) -> Option<i64> {
        conn.query_row(
            &format!("SELECT validity FROM {table} WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn test_add_unversioned_entity_to_version() {
        let conn = setup();
        let eid = insert_entity(&conn, "A");
        let vid = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();

        version_add_entity(&conn, vid, eid).unwrap();

        let v = get_validity(&conn, "kg_entities", eid);
        assert_eq!(v, Some(1)); // 0b1 — version 1 (bit 0)
    }

    #[test]
    fn test_add_entity_to_additional_version() {
        let conn = setup();
        let eid = insert_entity(&conn, "A");
        let v1 = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();
        let v2 = super::super::store::create_version(&conn, "v2", "main", None, None).unwrap();

        version_add_entity(&conn, v1, eid).unwrap();
        version_add_entity(&conn, v2, eid).unwrap();

        let v = get_validity(&conn, "kg_entities", eid);
        assert_eq!(v, Some(0b11)); // versions 1 and 2
    }

    #[test]
    fn test_remove_entity_from_one_of_multiple_versions() {
        let conn = setup();
        let eid = insert_entity(&conn, "A");
        let v1 = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();
        let v2 = super::super::store::create_version(&conn, "v2", "main", None, None).unwrap();

        version_add_entity(&conn, v1, eid).unwrap();
        version_add_entity(&conn, v2, eid).unwrap();
        version_remove_entity(&conn, v1, eid).unwrap();

        let v = get_validity(&conn, "kg_entities", eid);
        assert_eq!(v, Some(0b10)); // only version 2
    }

    #[test]
    fn test_remove_entity_from_only_version_returns_null() {
        let conn = setup();
        let eid = insert_entity(&conn, "A");
        let v1 = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();

        version_add_entity(&conn, v1, eid).unwrap();
        version_remove_entity(&conn, v1, eid).unwrap();

        let v = get_validity(&conn, "kg_entities", eid);
        assert_eq!(v, None); // back to unversioned
    }

    #[test]
    fn test_bulk_snapshot_entities() {
        let conn = setup();
        let e1 = insert_entity(&conn, "A");
        let e2 = insert_entity(&conn, "B");
        let vid = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();

        version_snapshot_entities(&conn, vid).unwrap();

        assert_eq!(get_validity(&conn, "kg_entities", e1), Some(1));
        assert_eq!(get_validity(&conn, "kg_entities", e2), Some(1));
    }

    #[test]
    fn test_add_relation_to_version() {
        let conn = setup();
        let e1 = insert_entity(&conn, "A");
        let e2 = insert_entity(&conn, "B");
        let rid = insert_relation(&conn, e1, e2);
        let vid = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();

        version_add_relation(&conn, vid, rid).unwrap();

        let v = get_validity(&conn, "kg_relations", rid);
        assert_eq!(v, Some(1));
    }

    #[test]
    fn test_nonexistent_entity_error() {
        let conn = setup();
        let vid = super::super::store::create_version(&conn, "v1", "main", None, None).unwrap();
        let err = version_add_entity(&conn, vid, 999).unwrap_err();
        assert!(matches!(err, crate::error::Error::EntityNotFound(999)));
    }

    #[test]
    fn test_nonexistent_version_error() {
        let conn = setup();
        let eid = insert_entity(&conn, "A");
        let err = version_add_entity(&conn, 999, eid).unwrap_err();
        assert!(matches!(err, crate::error::Error::VersionNotFound(999)));
    }
}
