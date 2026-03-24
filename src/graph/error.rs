use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Entity not found: {0}")]
    EntityNotFound(i64),
    
    #[error("Relation not found: {0}")]
    RelationNotFound(i64),
    
    #[error("Invalid property JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidVectorDimension { expected: usize, actual: usize },
}

pub type Result<T> = std::result::Result<T, GraphError>;