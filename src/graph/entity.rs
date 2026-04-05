//! Entity storage module for the knowledge graph.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::{Error, Result};

/// Represents an entity in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Option<i64>,
    pub entity_type: String,
    pub name: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

impl Entity {
    /// Create a new entity.
    pub fn new(entity_type: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: None,
            entity_type: entity_type.into(),
            name: name.into(),
            properties: HashMap::new(),
            created_at: None,
            updated_at: None,
        }
    }

    /// Create a new entity with properties.
    pub fn with_properties(
        entity_type: impl Into<String>,
        name: impl Into<String>,
        properties: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id: None,
            entity_type: entity_type.into(),
            name: name.into(),
            properties,
            created_at: None,
            updated_at: None,
        }
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

/// Insert a new entity into the database.
pub fn insert_entity(conn: &rusqlite::Connection, entity: &Entity) -> Result<i64> {
    let properties_json = serde_json::to_string(&entity.properties)?;

    conn.execute(
        r#"
        INSERT INTO kg_entities (entity_type, name, properties)
        VALUES (?1, ?2, ?3)
        "#,
        params![entity.entity_type, entity.name, properties_json],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get an entity by ID.
pub fn get_entity(conn: &rusqlite::Connection, id: i64) -> Result<Entity> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, entity_type, name, properties, created_at, updated_at
        FROM kg_entities
        WHERE id = ?1
        "#,
    )?;

    let entity = stmt.query_row(params![id], |row| {
        let properties_json: String = row.get(3)?;
        let properties: HashMap<String, serde_json::Value> =
            serde_json::from_str(&properties_json).unwrap_or_default();

        Ok(Entity {
            id: Some(row.get(0)?),
            entity_type: row.get(1)?,
            name: row.get(2)?,
            properties,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    Ok(entity)
}

/// List entities with optional filtering.
pub fn list_entities(
    conn: &rusqlite::Connection,
    entity_type: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<Entity>> {
    let mut query =
        "SELECT id, entity_type, name, properties, created_at, updated_at FROM kg_entities"
            .to_string();

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(et) = entity_type {
        query.push_str(" WHERE entity_type = ?1");
        params_vec.push(Box::new(et.to_string()));
    }

    query.push_str(" ORDER BY created_at DESC");

    if let Some(lim) = limit {
        query.push_str(" LIMIT ?");
        params_vec.push(Box::new(lim));
    }

    let mut stmt = conn.prepare(&query)?;

    // Convert boxed params to references for query_map
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let entities = stmt.query_map(params_refs.as_slice(), |row| {
        // Handle NULL properties column
        let properties_json: Option<String> = row.get(3)?;
        let properties: HashMap<String, serde_json::Value> = match properties_json {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => HashMap::new(),
        };

        Ok(Entity {
            id: Some(row.get(0)?),
            entity_type: row.get(1)?,
            name: row.get(2)?,
            properties,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    let mut result = Vec::new();
    for entity in entities {
        result.push(entity?);
    }

    Ok(result)
}

/// Update an entity.
pub fn update_entity(conn: &rusqlite::Connection, entity: &Entity) -> Result<()> {
    let id = entity.id.ok_or(Error::EntityNotFound(0))?;
    let properties_json = serde_json::to_string(&entity.properties)?;

    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Error::InvalidInput("system clock before UNIX epoch".to_string()))?
        .as_secs() as i64;

    let affected = conn.execute(
        r#"
        UPDATE kg_entities
        SET entity_type = ?1, name = ?2, properties = ?3, updated_at = ?4
        WHERE id = ?5
        "#,
        params![
            entity.entity_type,
            entity.name,
            properties_json,
            updated_at,
            id
        ],
    )?;

    if affected == 0 {
        return Err(Error::EntityNotFound(id));
    }

    Ok(())
}

/// Delete an entity by ID.
pub fn delete_entity(conn: &rusqlite::Connection, id: i64) -> Result<()> {
    let affected = conn.execute("DELETE FROM kg_entities WHERE id = ?1", params![id])?;

    if affected == 0 {
        return Err(Error::EntityNotFound(id));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_insert_entity() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity = Entity::new("paper", "Test Paper");
        let id = insert_entity(&conn, &entity).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_get_entity() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity = Entity::new("paper", "Test Paper");
        let id = insert_entity(&conn, &entity).unwrap();

        let retrieved = get_entity(&conn, id).unwrap();
        assert_eq!(retrieved.id, Some(id));
        assert_eq!(retrieved.entity_type, "paper");
        assert_eq!(retrieved.name, "Test Paper");
    }

    #[test]
    fn test_list_entities() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();
        insert_entity(&conn, &Entity::new("paper", "Paper 2")).unwrap();
        insert_entity(&conn, &Entity::new("skill", "Skill 1")).unwrap();

        let papers = list_entities(&conn, Some("paper"), None).unwrap();
        assert_eq!(papers.len(), 2);

        let all = list_entities(&conn, None, Some(2)).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_entity_properties() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let mut entity = Entity::new("paper", "Test Paper");
        entity.set_property("author", serde_json::json!("John Doe"));
        entity.set_property("year", serde_json::json!(2024));

        let id = insert_entity(&conn, &entity).unwrap();

        let retrieved = get_entity(&conn, id).unwrap();
        assert_eq!(
            retrieved.get_property("author"),
            Some(&serde_json::json!("John Doe"))
        );
        assert_eq!(
            retrieved.get_property("year"),
            Some(&serde_json::json!(2024))
        );
    }
}
