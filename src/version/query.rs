//! Version-filtered queries for entities, relations, and neighbor traversal.

use std::collections::{HashSet, VecDeque};

use rusqlite::params;

use super::store;
use crate::error::{Error, Result};
use crate::graph::entity::Entity;
use crate::graph::relation::Relation;

/// Get all entities in a specific version.
pub fn version_entities(
    conn: &rusqlite::Connection,
    version_id: i64,
    entity_type: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<Entity>> {
    let bit = store::version_bit_for(conn, version_id)?;
    let mut query = String::from(
        "SELECT id, entity_type, name, properties, created_at, updated_at \
         FROM kg_entities WHERE (validity & ?1) != 0",
    );

    let mut param_idx = 2;
    if entity_type.is_some() {
        query.push_str(&format!(" AND entity_type = ?{param_idx}"));
        param_idx += 1;
    }

    if limit.is_some() {
        query.push_str(&format!(" LIMIT ?{param_idx}"));
    }

    let mut stmt = conn.prepare(&query)?;

    let mut param_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(bit)];
    if let Some(et) = entity_type {
        param_vec.push(Box::new(et.to_string()));
    }
    if let Some(lim) = limit {
        param_vec.push(Box::new(lim));
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = param_vec.iter().map(|p| p.as_ref()).collect();

    let entities = stmt.query_map(params_refs.as_slice(), row_to_entity)?;

    let mut result = Vec::new();
    for e in entities {
        result.push(e?);
    }
    Ok(result)
}

/// Get all relations in a specific version.
pub fn version_relations(
    conn: &rusqlite::Connection,
    version_id: i64,
    rel_type: Option<&str>,
    source_id: Option<i64>,
    target_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<Relation>> {
    let bit = store::version_bit_for(conn, version_id)?;
    let mut query = String::from(
        "SELECT id, source_id, target_id, rel_type, weight, properties, created_at \
         FROM kg_relations WHERE (validity & ?1) != 0",
    );

    let mut param_idx = 2;
    if rel_type.is_some() {
        query.push_str(&format!(" AND rel_type = ?{param_idx}"));
        param_idx += 1;
    }
    if source_id.is_some() {
        query.push_str(&format!(" AND source_id = ?{param_idx}"));
        param_idx += 1;
    }
    if target_id.is_some() {
        query.push_str(&format!(" AND target_id = ?{param_idx}"));
        param_idx += 1;
    }
    if limit.is_some() {
        query.push_str(&format!(" LIMIT ?{param_idx}"));
    }

    let mut stmt = conn.prepare(&query)?;

    let mut param_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(bit)];
    if let Some(rt) = rel_type {
        param_vec.push(Box::new(rt.to_string()));
    }
    if let Some(sid) = source_id {
        param_vec.push(Box::new(sid));
    }
    if let Some(tid) = target_id {
        param_vec.push(Box::new(tid));
    }
    if let Some(lim) = limit {
        param_vec.push(Box::new(lim));
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = param_vec.iter().map(|p| p.as_ref()).collect();

    let relations = stmt.query_map(params_refs.as_slice(), row_to_relation)?;

    let mut result = Vec::new();
    for r in relations {
        result.push(r?);
    }
    Ok(result)
}

/// Version-aware neighbor traversal. Both the entity and the connecting relation
/// must exist in the specified version.
pub fn version_neighbors(
    conn: &rusqlite::Connection,
    entity_id: i64,
    version_id: i64,
    depth: u32,
) -> Result<Vec<crate::graph::relation::Neighbor>> {
    if depth == 0 {
        return Ok(Vec::new());
    }
    if depth > 5 {
        return Err(Error::InvalidDepth(depth));
    }

    let bit = store::version_bit_for(conn, version_id)?;
    store::ensure_entity_exists(conn, entity_id)?;

    // The traversal only crosses edges/nodes that live in this version; a start
    // entity outside the version has no in-version neighborhood.
    if !entity_in_version(conn, entity_id, bit)? {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    visited.insert(entity_id);

    let direct = get_direct_version_relations(conn, entity_id, bit)?;
    for (relation, neighbor_entity) in direct {
        let nid = neighbor_entity.id.ok_or(Error::EntityNotFound(0))?;
        if !visited.contains(&nid) {
            visited.insert(nid);
            queue.push_back((nid, 1));
            result.push(crate::graph::relation::Neighbor {
                entity: neighbor_entity,
                relation,
            });
        }
    }

    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        let relations = get_direct_version_relations(conn, current_id, bit)?;
        for (relation, neighbor_entity) in relations {
            let nid = neighbor_entity.id.ok_or(Error::EntityNotFound(0))?;
            if !visited.contains(&nid) {
                visited.insert(nid);
                queue.push_back((nid, current_depth + 1));
                result.push(crate::graph::relation::Neighbor {
                    entity: neighbor_entity,
                    relation,
                });
            }
        }
    }

    Ok(result)
}

/// Whether an entity's validity bitstring includes the given version bit.
fn entity_in_version(conn: &rusqlite::Connection, entity_id: i64, bit: i64) -> Result<bool> {
    let present: bool = conn.query_row(
        "SELECT COALESCE((validity & ?1) != 0, 0) FROM kg_entities WHERE id = ?2",
        params![bit, entity_id],
        |r| r.get(0),
    )?;
    Ok(present)
}

/// Get direct version-filtered relations for an entity (both directions).
fn get_direct_version_relations(
    conn: &rusqlite::Connection,
    entity_id: i64,
    bit: i64,
) -> Result<Vec<(Relation, Entity)>> {
    let mut result = Vec::new();

    // Outgoing: entity_id is source
    let mut stmt = conn.prepare(
        "SELECT r.id, r.source_id, r.target_id, r.rel_type, r.weight, r.properties, r.created_at,
                e.id, e.entity_type, e.name, e.properties, e.created_at, e.updated_at
         FROM kg_relations r
         JOIN kg_entities e ON r.target_id = e.id
         WHERE r.source_id = ?1 AND (r.validity & ?2) != 0 AND (e.validity & ?2) != 0",
    )?;
    let rows = stmt.query_map(params![entity_id, bit], |row| {
        Ok((row_to_relation(row)?, row_to_entity_offset(row, 7)?))
    })?;
    for row in rows {
        result.push(row?);
    }

    // Incoming: entity_id is target
    let mut stmt = conn.prepare(
        "SELECT r.id, r.source_id, r.target_id, r.rel_type, r.weight, r.properties, r.created_at,
                e.id, e.entity_type, e.name, e.properties, e.created_at, e.updated_at
         FROM kg_relations r
         JOIN kg_entities e ON r.source_id = e.id
         WHERE r.target_id = ?1 AND (r.validity & ?2) != 0 AND (e.validity & ?2) != 0",
    )?;
    let rows = stmt.query_map(params![entity_id, bit], |row| {
        Ok((row_to_relation(row)?, row_to_entity_offset(row, 7)?))
    })?;
    for row in rows {
        result.push(row?);
    }

    Ok(result)
}

fn row_to_entity(row: &rusqlite::Row) -> rusqlite::Result<Entity> {
    let props_json: Option<String> = row.get(3)?;
    let properties = props_json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    Ok(Entity {
        id: Some(row.get(0)?),
        entity_type: row.get(1)?,
        name: row.get(2)?,
        properties,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn row_to_entity_offset(row: &rusqlite::Row, offset: usize) -> rusqlite::Result<Entity> {
    let props_json: Option<String> = row.get(offset + 3)?;
    let properties = props_json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    Ok(Entity {
        id: Some(row.get(offset)?),
        entity_type: row.get(offset + 1)?,
        name: row.get(offset + 2)?,
        properties,
        created_at: row.get(offset + 4)?,
        updated_at: row.get(offset + 5)?,
    })
}

fn row_to_relation(row: &rusqlite::Row) -> rusqlite::Result<Relation> {
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

    fn add_relation(conn: &Connection, src: i64, tgt: i64, rt: &str) -> i64 {
        conn.execute(
            "INSERT INTO kg_relations (source_id, target_id, rel_type) VALUES (?1, ?2, ?3)",
            rusqlite::params![src, tgt, rt],
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
    fn test_entities_in_version() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let v1 = make_version(&conn, "v1");

        set_validity(&conn, "kg_entities", e1, 0b01); // v1 only
        set_validity(&conn, "kg_entities", e2, 0b01); // v1 only
        set_validity(&conn, "kg_entities", e3, 0b10); // v2 only (doesn't exist yet)

        let ents = version_entities(&conn, v1, None, None).unwrap();
        assert_eq!(ents.len(), 2);
        let names: Vec<&str> = ents.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"A"));
        assert!(names.contains(&"B"));
    }

    #[test]
    fn test_entities_with_type_filter() {
        let conn = setup();
        conn.execute(
            "INSERT INTO kg_entities (entity_type, name) VALUES ('paper', 'P1')",
            [],
        )
        .unwrap();
        let e2 = add_entity(&conn, "S1");
        let v1 = make_version(&conn, "v1");

        set_validity(&conn, "kg_entities", e2, 0b01);
        conn.execute("UPDATE kg_entities SET validity = 1 WHERE name = 'P1'", [])
            .unwrap();

        let papers = version_entities(&conn, v1, Some("paper"), None).unwrap();
        assert_eq!(papers.len(), 1);
        assert_eq!(papers[0].name, "P1");
    }

    #[test]
    fn test_relations_in_version() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let r1 = add_relation(&conn, e1, e2, "cites");
        let v1 = make_version(&conn, "v1");

        set_validity(&conn, "kg_relations", r1, 0b01);

        let rels = version_relations(&conn, v1, None, None, None, None).unwrap();
        assert_eq!(rels.len(), 1);
    }

    #[test]
    fn test_relations_with_type_filter() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let r1 = add_relation(&conn, e1, e2, "cites");
        let r2 = add_relation(&conn, e2, e1, "related");
        let v1 = make_version(&conn, "v1");

        set_validity(&conn, "kg_relations", r1, 0b01);
        set_validity(&conn, "kg_relations", r2, 0b01);

        let cites = version_relations(&conn, v1, Some("cites"), None, None, None).unwrap();
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].rel_type, "cites");
    }

    #[test]
    fn test_version_neighbors() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let r1 = add_relation(&conn, e1, e2, "knows");
        let r2 = add_relation(&conn, e1, e3, "knows");
        let v1 = make_version(&conn, "v1");

        set_validity(&conn, "kg_entities", e1, 0b01);
        set_validity(&conn, "kg_entities", e2, 0b01);
        set_validity(&conn, "kg_entities", e3, 0b01);
        set_validity(&conn, "kg_relations", r1, 0b01);
        set_validity(&conn, "kg_relations", r2, 0b01);

        let neighbors = version_neighbors(&conn, e1, v1, 1).unwrap();
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn test_version_neighbors_excludes_non_version_entity() {
        let conn = setup();
        let e1 = add_entity(&conn, "A");
        let e2 = add_entity(&conn, "B");
        let e3 = add_entity(&conn, "C");
        let r1 = add_relation(&conn, e1, e2, "knows");
        let r2 = add_relation(&conn, e1, e3, "knows");
        let v1 = make_version(&conn, "v1");

        set_validity(&conn, "kg_entities", e1, 0b01);
        set_validity(&conn, "kg_entities", e2, 0b01);
        // e3 is NOT in v1
        set_validity(&conn, "kg_relations", r1, 0b01);
        set_validity(&conn, "kg_relations", r2, 0b01);

        let neighbors = version_neighbors(&conn, e1, v1, 1).unwrap();
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].entity.name, "B");
    }
}
