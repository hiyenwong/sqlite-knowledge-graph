pub mod graph;
pub mod error;

pub use error::GraphError;

/// Knowledge Graph entity
#[derive(Debug, Clone)]
pub struct Entity {
    pub id: i64,
    pub entity_type: String,
    pub name: String,
    pub properties: serde_json::Value,
}

/// Knowledge Graph relation
#[derive(Debug, Clone)]
pub struct Relation {
    pub id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub relation_type: String,
    pub weight: f64,
    pub properties: serde_json::Value,
}

/// Knowledge Graph storage
pub struct KnowledgeGraph {
    // TODO: Implement storage
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {}
    }
    
    pub fn insert_entity(&mut self, entity_type: &str, name: &str, properties: &str) -> Result<i64, GraphError> {
        // TODO: Implement
        Ok(1)
    }
    
    pub fn insert_relation(&mut self, source: i64, target: i64, rel_type: &str, weight: f64) -> Result<i64, GraphError> {
        // TODO: Implement
        Ok(1)
    }
    
    pub fn get_neighbors(&self, entity_id: i64, depth: i32) -> Vec<Entity> {
        // TODO: Implement
        vec![]
    }
}