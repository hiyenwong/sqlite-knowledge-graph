//! Relation storage module for the knowledge graph.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::error::{Error, Result};
use crate::graph::entity::Entity;

/// Represents a relation between entities in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: Option<i64>,
    pub source_id: i64,
    pub target_id: i64,
    pub rel_type: String,
    pub weight: f64,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: Option<i64>,
}

impl Relation {
    /// Create a new relation.
    pub fn new(
        source_id: i64,
        target_id: i64,
        rel_type: impl Into<String>,
        weight: f64,
    ) -> Result<Self> {
        if !(0.0..=1.0).contains(&weight) {
            return Err(Error::InvalidWeight(weight));
        }

        Ok(Self {
            id: None,
            source_id,
            target_id,
            rel_type: rel_type.into(),
            weight,
            properties: HashMap::new(),
            created_at: None,
        })
    }

    /// Set a property.
    pub fn set_property(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.properties.insert(key.into(), value);
    }

    /// Get a property.
    pub fn get_property(&self, key: &str) -> Option<&serde_json::Value> {
        self.properties.get(key)
    }
}

/// Represents a neighbor in a graph traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbor {
    pub entity: Entity,
    pub relation: Relation,
}

/// Insert a new relation into the database.
pub fn insert_relation(conn: &rusqlite::Connection, relation: &Relation) -> Result<i64> {
    // Validate entities exist
    crate::graph::entity::get_entity(conn, relation.source_id)?;
    crate::graph::entity::get_entity(conn, relation.target_id)?;

    let properties_json = serde_json::to_string(&relation.properties)?;

    conn.execute(
        r#"
        INSERT INTO kg_relations (source_id, target_id, rel_type, weight, properties)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            relation.source_id,
            relation.target_id,
            relation.rel_type,
            relation.weight,
            properties_json
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get neighbors of an entity using BFS traversal.
pub fn get_neighbors(
    conn: &rusqlite::Connection,
    entity_id: i64,
    depth: u32,
) -> Result<Vec<Neighbor>> {
    if depth == 0 {
        return Ok(Vec::new());
    }

    if depth > 5 {
        return Err(Error::InvalidDepth(depth));
    }

    // Validate entity exists
    crate::graph::entity::get_entity(conn, entity_id)?;

    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = VecDeque::new();

    // Start with direct neighbors
    visited.insert(entity_id);
    let direct_relations = get_direct_relations(conn, entity_id)?;

    for (relation, neighbor_entity) in direct_relations {
        let neighbor_id = neighbor_entity.id.ok_or(Error::EntityNotFound(0))?;

        if !visited.contains(&neighbor_id) {
            visited.insert(neighbor_id);
            queue.push_back((neighbor_id, 1));
            result.push(Neighbor {
                entity: neighbor_entity,
                relation,
            });
        }
    }

    // BFS traversal
    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }

        let relations = get_direct_relations(conn, current_id)?;

        for (relation, neighbor_entity) in relations {
            let neighbor_id = neighbor_entity.id.ok_or(Error::EntityNotFound(0))?;

            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id);
                queue.push_back((neighbor_id, current_depth + 1));
                result.push(Neighbor {
                    entity: neighbor_entity,
                    relation,
                });
            }
        }
    }

    Ok(result)
}

/// Get direct relations for an entity (both incoming and outgoing).
fn get_direct_relations(
    conn: &rusqlite::Connection,
    entity_id: i64,
) -> Result<Vec<(Relation, Entity)>> {
    let mut result = Vec::new();

    // Outgoing relations (entity_id is source)
    let mut stmt = conn.prepare(
        r#"
        SELECT r.id, r.source_id, r.target_id, r.rel_type, r.weight, r.properties, r.created_at,
               e.id, e.entity_type, e.name, e.properties, e.created_at, e.updated_at
        FROM kg_relations r
        JOIN kg_entities e ON r.target_id = e.id
        WHERE r.source_id = ?1
        "#,
    )?;

    let rows = stmt.query_map(params![entity_id], |row| {
        let properties_json: String = row.get(5)?;
        let properties: HashMap<String, serde_json::Value> =
            serde_json::from_str(&properties_json).unwrap_or_default();

        let entity_props_json: String = row.get(10)?;
        let entity_props: HashMap<String, serde_json::Value> =
            serde_json::from_str(&entity_props_json).unwrap_or_default();

        Ok((
            Relation {
                id: Some(row.get(0)?),
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                rel_type: row.get(3)?,
                weight: crate::row_get_weight(row, 4)?,
                properties,
                created_at: row.get(6)?,
            },
            Entity {
                id: Some(row.get(7)?),
                entity_type: row.get(8)?,
                name: row.get(9)?,
                properties: entity_props,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            },
        ))
    })?;

    for row in rows {
        result.push(row?);
    }

    // Incoming relations (entity_id is target)
    let mut stmt = conn.prepare(
        r#"
        SELECT r.id, r.source_id, r.target_id, r.rel_type, r.weight, r.properties, r.created_at,
               e.id, e.entity_type, e.name, e.properties, e.created_at, e.updated_at
        FROM kg_relations r
        JOIN kg_entities e ON r.source_id = e.id
        WHERE r.target_id = ?1
        "#,
    )?;

    let rows = stmt.query_map(params![entity_id], |row| {
        let properties_json: String = row.get(5)?;
        let properties: HashMap<String, serde_json::Value> =
            serde_json::from_str(&properties_json).unwrap_or_default();

        let entity_props_json: String = row.get(10)?;
        let entity_props: HashMap<String, serde_json::Value> =
            serde_json::from_str(&entity_props_json).unwrap_or_default();

        Ok((
            Relation {
                id: Some(row.get(0)?),
                source_id: row.get(1)?,
                target_id: row.get(2)?,
                rel_type: row.get(3)?,
                weight: crate::row_get_weight(row, 4)?,
                properties,
                created_at: row.get(6)?,
            },
            Entity {
                id: Some(row.get(7)?),
                entity_type: row.get(8)?,
                name: row.get(9)?,
                properties: entity_props,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            },
        ))
    })?;

    for row in rows {
        result.push(row?);
    }

    Ok(result)
}

/// Get relations by source ID.
pub fn get_relations_by_source(
    conn: &rusqlite::Connection,
    source_id: i64,
) -> Result<Vec<Relation>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, source_id, target_id, rel_type, weight, properties, created_at
        FROM kg_relations
        WHERE source_id = ?1
        "#,
    )?;

    let relations = stmt.query_map(params![source_id], |row| {
        let properties_json: String = row.get(5)?;
        let properties: HashMap<String, serde_json::Value> =
            serde_json::from_str(&properties_json).unwrap_or_default();

        Ok(Relation {
            id: Some(row.get(0)?),
            source_id: row.get(1)?,
            target_id: row.get(2)?,
            rel_type: row.get(3)?,
            weight: row.get(4)?,
            properties,
            created_at: row.get(6)?,
        })
    })?;

    let mut result = Vec::new();
    for rel in relations {
        result.push(rel?);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::entity::{insert_entity, Entity};
    use rusqlite::Connection;

    #[test]
    fn test_insert_relation() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity1_id = insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();
        let entity2_id = insert_entity(&conn, &Entity::new("paper", "Paper 2")).unwrap();

        let relation = Relation::new(entity1_id, entity2_id, "cites", 0.8).unwrap();
        let id = insert_relation(&conn, &relation).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_get_neighbors_depth_1() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity1_id = insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();
        let entity2_id = insert_entity(&conn, &Entity::new("paper", "Paper 2")).unwrap();
        let entity3_id = insert_entity(&conn, &Entity::new("paper", "Paper 3")).unwrap();

        let relation = Relation::new(entity1_id, entity2_id, "cites", 0.8).unwrap();
        insert_relation(&conn, &relation).unwrap();

        let relation = Relation::new(entity2_id, entity3_id, "cites", 0.9).unwrap();
        insert_relation(&conn, &relation).unwrap();

        let neighbors = get_neighbors(&conn, entity1_id, 1).unwrap();
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].entity.name, "Paper 2");
    }

    #[test]
    fn test_get_neighbors_depth_2() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity1_id = insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();
        let entity2_id = insert_entity(&conn, &Entity::new("paper", "Paper 2")).unwrap();
        let entity3_id = insert_entity(&conn, &Entity::new("paper", "Paper 3")).unwrap();

        let relation = Relation::new(entity1_id, entity2_id, "cites", 0.8).unwrap();
        insert_relation(&conn, &relation).unwrap();

        let relation = Relation::new(entity2_id, entity3_id, "cites", 0.9).unwrap();
        insert_relation(&conn, &relation).unwrap();

        let neighbors = get_neighbors(&conn, entity1_id, 2).unwrap();
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.iter().any(|n| n.entity.name == "Paper 2"));
        assert!(neighbors.iter().any(|n| n.entity.name == "Paper 3"));
    }

    #[test]
    fn test_invalid_weight() {
        let relation = Relation::new(1, 2, "test", 1.5);
        assert!(relation.is_err());
    }

    #[test]
    fn test_invalid_depth() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity1_id = insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();

        let result = get_neighbors(&conn, entity1_id, 10);
        assert!(result.is_err());
    }
}
