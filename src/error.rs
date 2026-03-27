//! Error types for sqlite-knowledge-graph.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SQLite error: {0}")]
    SQLite(#[from] rusqlite::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Entity not found: {0}")]
    EntityNotFound(i64),

    #[error("Relation not found: {0}")]
    RelationNotFound(i64),

    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidVectorDimension { expected: usize, actual: usize },

    #[error("Invalid depth: {0}")]
    InvalidDepth(u32),

    #[error("Invalid weight: {0}")]
    InvalidWeight(f64),

    #[error("Invalid arity: {0} (minimum 2 entities required)")]
    InvalidArity(usize),

    #[error("Hyperedge not found: {0}")]
    HyperedgeNotFound(i64),

    #[error("Invalid entity type: {0}")]
    InvalidEntityType(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Database is closed")]
    DatabaseClosed,

    #[error("Other error: {0}")]
    Other(String),
}
