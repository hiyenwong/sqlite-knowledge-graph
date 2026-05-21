//! Version merge operations — union and intersection strategies.

use rusqlite::params;

use super::store;
use super::MergeStrategy;
use crate::error::{Error, Result};

/// Merge two or more versions into a new version.
///
/// The entire merge — new-version creation, `is_merged` flag, and all validity
/// updates — runs in one transaction, so a failure leaves no half-built version.
pub fn version_merge(
    conn: &rusqlite::Connection,
    source_ids: &[i64],
    target_name: &str,
    strategy: MergeStrategy,
) -> Result<i64> {
    if source_ids.len() < 2 {
        return Err(Error::InvalidMerge(
            "merge requires at least 2 source versions".to_string(),
        ));
    }

    // Combine source slots into one mask, validating each version exists.
    let mut source_mask: i64 = 0;
    for &sid in source_ids {
        source_mask |= store::version_bit_for(conn, sid)?;
    }

    let tx = conn.unchecked_transaction()?;

    let new_id = store::create_version(
        &tx,
        target_name,
        "main",
        Some(source_ids[0]),
        Some(&format!("merge of {:?}", source_ids)),
    )?;
    tx.execute(
        "UPDATE kg_versions SET is_merged = 1 WHERE id = ?1",
        [new_id],
    )?;

    let new_bit = store::version_bit_for(&tx, new_id)?;
    match strategy {
        MergeStrategy::Union => apply_merge(&tx, new_bit, source_mask, MergeStrategy::Union)?,
        MergeStrategy::Intersection => {
            apply_merge(&tx, new_bit, source_mask, MergeStrategy::Intersection)?
        }
    }

    tx.commit()?;
    Ok(new_id)
}

/// Set `new_bit` on every entity and relation that matches the strategy:
/// - Union: validity overlaps ANY source bit → `(validity & mask) != 0`
/// - Intersection: validity covers ALL source bits → `(validity & mask) = mask`
fn apply_merge(
    conn: &rusqlite::Connection,
    new_bit: i64,
    source_mask: i64,
    strategy: MergeStrategy,
) -> Result<()> {
    let predicate = match strategy {
        MergeStrategy::Union => "(validity & ?2) != 0",
        MergeStrategy::Intersection => "(validity & ?2) = ?2",
    };

    for table in ["kg_entities", "kg_relations"] {
        // `table` is a hard-coded literal, never user input.
        conn.execute(
            &format!(
                "UPDATE {table} SET validity = validity | ?1 \
                 WHERE validity IS NOT NULL AND {predicate}"
            ),
            params![new_bit, source_mask],
        )?;
    }
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

    fn add_entity(conn: &Connection, name: &str) -> i64 {
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name) VALUES ('test', ?1)",
            [name],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn add_relation(conn: &Connection, src: i64, tgt: i64) -> i64 {
        conn.execute(
            "INSERT INTO kg_relations (source_id, target_id, rel_type) VALUES (?1, ?2, 'rel')",
            rusqlite::params![src, tgt],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn make_version(conn: &Connection, name: &str) -> i64 {
        super::super::store::create_version(conn, name, "main", None, None).unwrap()
    }

    fn set_validity(conn: &Connection, table: &str, id: i64, val: i64) {
        conn.execute(
            &format!("UPDATE {table} SET validity = ?1 WHERE id = ?2"),
            rusqlite::params![val, id],
        )
        .unwrap();
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
    fn test_union_merge() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        // v1: A, B
        set_validity(&conn, "kg_entities", e1, 0b01);
        set_validity(&conn, "kg_entities", e2, 0b01);
        // v2: B, C
        set_validity(&conn, "kg_entities", e3, 0b10);
        conn.execute(
            "UPDATE kg_entities SET validity = validity | 2 WHERE id = ?1",
            [e2],
        )
        .unwrap(); // B in both: 0b11

        let merged = version_merge(&conn, &[v1, v2], "merged-union", MergeStrategy::Union).unwrap();
        let mb = store::version_bit_for(&conn, merged).unwrap();

        // All three should have the merged version's bit
        assert!(get_validity(&conn, "kg_entities", e1).unwrap() & mb != 0);
        assert!(get_validity(&conn, "kg_entities", e2).unwrap() & mb != 0);
        assert!(get_validity(&conn, "kg_entities", e3).unwrap() & mb != 0);
    }

    #[test]
    fn test_intersection_merge() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        // v1: A, B
        set_validity(&conn, "kg_entities", e1, 0b01);
        set_validity(&conn, "kg_entities", e2, 0b11); // B in both
                                                      // v2: B, C
        set_validity(&conn, "kg_entities", e3, 0b10);

        let merged = version_merge(
            &conn,
            &[v1, v2],
            "merged-intersect",
            MergeStrategy::Intersection,
        )
        .unwrap();
        let mb = store::version_bit_for(&conn, merged).unwrap();

        // Only B should be in the intersection
        assert!(get_validity(&conn, "kg_entities", e1).unwrap() & mb == 0);
        assert!(get_validity(&conn, "kg_entities", e2).unwrap() & mb != 0);
        assert!(get_validity(&conn, "kg_entities", e3).unwrap() & mb == 0);
    }

    #[test]
    fn test_merge_applies_to_relations() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let r1 = add_relation(&conn, e1, e2);
        let r2 = add_relation(&conn, e2, e3);
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        set_validity(&conn, "kg_relations", r1, 0b11); // both versions
        set_validity(&conn, "kg_relations", r2, 0b10); // v2 only

        // Union: both relations carry the merged bit; intersection: only r1.
        let mu = version_merge(&conn, &[v1, v2], "u", MergeStrategy::Union).unwrap();
        let mub = store::version_bit_for(&conn, mu).unwrap();
        assert!(get_validity(&conn, "kg_relations", r1).unwrap() & mub != 0);
        assert!(get_validity(&conn, "kg_relations", r2).unwrap() & mub != 0);

        let mi = version_merge(&conn, &[v1, v2], "i", MergeStrategy::Intersection).unwrap();
        let mib = store::version_bit_for(&conn, mi).unwrap();
        assert!(get_validity(&conn, "kg_relations", r1).unwrap() & mib != 0);
        assert!(get_validity(&conn, "kg_relations", r2).unwrap() & mib == 0);
    }

    #[test]
    fn test_merge_creates_version_row() {
        let conn = setup();
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        let merged = version_merge(&conn, &[v1, v2], "merged", MergeStrategy::Union).unwrap();

        let v = super::super::store::get_version(&conn, merged).unwrap();
        assert_eq!(v.name, "merged");
        assert_eq!(v.parent_id, Some(v1));
        assert!(v.is_merged);
    }

    #[test]
    fn test_delete_parent_after_merge_reclaims_slot() {
        // Production connections enable foreign keys; ON DELETE SET NULL only
        // fires under enforcement, so opt in explicitly here.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");
        let merged = version_merge(&conn, &[v1, v2], "m", MergeStrategy::Union).unwrap();
        let freed_bit = store::version_bit_for(&conn, v1).unwrap();

        // v1 is the merged version's parent; deleting it must succeed (not RESTRICT).
        store::delete_version(&conn, v1).unwrap();

        // The merged child's parent_id was nulled, not left dangling.
        assert_eq!(store::get_version(&conn, merged).unwrap().parent_id, None);

        // v1's slot is reclaimed: the next version reuses that exact bit.
        let v3 = make_version(&conn, "v3");
        assert_eq!(store::version_bit_for(&conn, v3).unwrap(), freed_bit);
    }

    #[test]
    fn test_merge_single_source_rejected() {
        let conn = setup();
        let v1 = make_version(&conn, "v1");
        let err = version_merge(&conn, &[v1], "bad", MergeStrategy::Union).unwrap_err();
        assert!(matches!(err, Error::InvalidMerge(_)));
    }

    #[test]
    fn test_merge_nonexistent_version_rejected() {
        let conn = setup();
        let v1 = make_version(&conn, "v1");
        let err = version_merge(&conn, &[v1, 999], "bad", MergeStrategy::Union).unwrap_err();
        assert!(matches!(err, Error::VersionNotFound(999)));
    }
}
