//! Version comparison and entity history.

use super::store;
use super::Version;
use super::VersionDiff;
use crate::error::Result;
use crate::graph::entity::Entity;
use crate::graph::relation::Relation;

/// Compare two versions and return added/removed/common entities and relations.
pub fn version_compare(conn: &rusqlite::Connection, v1_id: i64, v2_id: i64) -> Result<VersionDiff> {
    store::ensure_version_exists(conn, v1_id)?;
    store::ensure_version_exists(conn, v2_id)?;

    let ents1 = entity_ids_in_version(conn, v1_id)?;
    let ents2 = entity_ids_in_version(conn, v2_id)?;

    let rels1 = relation_ids_in_version(conn, v1_id)?;
    let rels2 = relation_ids_in_version(conn, v2_id)?;

    let (added_e, removed_e, common_e) = partition(&ents1, &ents2);
    let (added_r, removed_r, common_r) = partition(&rels1, &rels2);

    Ok(VersionDiff {
        added_entities: load_entities(conn, &added_e)?,
        removed_entities: load_entities(conn, &removed_e)?,
        common_entities: load_entities(conn, &common_e)?,
        added_relations: load_relations(conn, &added_r)?,
        removed_relations: load_relations(conn, &removed_r)?,
        common_relations: load_relations(conn, &common_r)?,
    })
}

/// Return all versions that contain a given entity.
pub fn version_entity_history(conn: &rusqlite::Connection, entity_id: i64) -> Result<Vec<Version>> {
    store::ensure_entity_exists(conn, entity_id)?;

    let validity: Option<i64> = conn.query_row(
        "SELECT validity FROM kg_entities WHERE id = ?1",
        [entity_id],
        |r| r.get(0),
    )?;

    let Some(bits) = validity else {
        return Ok(Vec::new());
    };

    // Resolve the set bits back to their owning versions (newest first).
    store::versions_for_bits(conn, bits)
}

fn entity_ids_in_version(conn: &rusqlite::Connection, version_id: i64) -> Result<Vec<i64>> {
    let bit = store::version_bit_for(conn, version_id)?;
    let mut stmt = conn.prepare("SELECT id FROM kg_entities WHERE (validity & ?1) != 0")?;
    let ids: Vec<i64> = stmt
        .query_map([bit], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(ids)
}

fn relation_ids_in_version(conn: &rusqlite::Connection, version_id: i64) -> Result<Vec<i64>> {
    let bit = store::version_bit_for(conn, version_id)?;
    let mut stmt = conn.prepare("SELECT id FROM kg_relations WHERE (validity & ?1) != 0")?;
    let ids: Vec<i64> = stmt
        .query_map([bit], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(ids)
}

fn partition(ids1: &[i64], ids2: &[i64]) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let set1: std::collections::HashSet<i64> = ids1.iter().copied().collect();
    let set2: std::collections::HashSet<i64> = ids2.iter().copied().collect();

    let added: Vec<i64> = set2.difference(&set1).copied().collect();
    let removed: Vec<i64> = set1.difference(&set2).copied().collect();
    let common: Vec<i64> = set1.intersection(&set2).copied().collect();

    (added, removed, common)
}

fn load_entities(conn: &rusqlite::Connection, ids: &[i64]) -> Result<Vec<Entity>> {
    let mut result = Vec::new();
    for &id in ids {
        result.push(crate::graph::entity::get_entity(conn, id)?);
    }
    Ok(result)
}

fn load_relations(conn: &rusqlite::Connection, ids: &[i64]) -> Result<Vec<Relation>> {
    let mut result = Vec::new();
    for &id in ids {
        let mut stmt = conn.prepare(
            "SELECT id, source_id, target_id, rel_type, weight, properties, created_at \
             FROM kg_relations WHERE id = ?1",
        )?;
        let rel = stmt.query_row([id], |row| {
            let props_json: Option<String> = row.get(5)?;
            let properties = props_json
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();
            Ok(Relation {
                id: Some(row.get(0)?),
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                rel_type: row.get(3)?,
                weight: crate::row_get_weight(row, 4)?,
                properties,
                created_at: row.get(6)?,
            })
        })?;
        result.push(rel);
    }
    Ok(result)
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

    #[test]
    fn test_added_entities() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        set_validity(&conn, "kg_entities", e1, 0b01); // v1
        set_validity(&conn, "kg_entities", e2, 0b11); // v1 + v2
        set_validity(&conn, "kg_entities", e3, 0b10); // v2

        let diff = version_compare(&conn, v1, v2).unwrap();
        assert_eq!(diff.added_entities.len(), 1);
        assert_eq!(diff.added_entities[0].name, "C");
        assert_eq!(diff.removed_entities.len(), 1);
        assert_eq!(diff.removed_entities[0].name, "A");
        assert_eq!(diff.common_entities.len(), 1);
        assert_eq!(diff.common_entities[0].name, "B");
    }

    #[test]
    fn test_relations_diff() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let r1 = add_relation(&conn, e1, e2);
        let r2 = add_relation(&conn, e2, e3);
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        set_validity(&conn, "kg_relations", r1, 0b11); // both
        set_validity(&conn, "kg_relations", r2, 0b10); // v2 only

        let diff = version_compare(&conn, v1, v2).unwrap();
        assert_eq!(diff.added_relations.len(), 1);
        assert_eq!(diff.common_relations.len(), 1);
    }

    #[test]
    fn test_entity_history_multi_version() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let v1 = make_version(&conn, "v1");
        let v2 = make_version(&conn, "v2");

        // validity = bit for v1 | bit for v2
        let validity = super::super::store::version_bit_for(&conn, v1).unwrap()
            | super::super::store::version_bit_for(&conn, v2).unwrap();
        set_validity(&conn, "kg_entities", e1, validity);

        let history = version_entity_history(&conn, e1).unwrap();
        assert_eq!(history.len(), 2);
        let ids: Vec<i64> = history.iter().map(|v| v.id).collect();
        assert!(ids.contains(&v1));
        assert!(ids.contains(&v2));
    }

    #[test]
    fn test_entity_history_unversioned() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");

        let history = version_entity_history(&conn, e1).unwrap();
        assert!(history.is_empty());
    }
}
