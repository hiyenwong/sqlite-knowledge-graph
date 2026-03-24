use thiserror::Error;

#[derive(Error, Debug)]
pub enum VectorError {
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { expected: usize, actual: usize },
    
    #[error("Vector not found: {0}")]
    VectorNotFound(i64),
    
    #[error("Index error: {0}")]
    IndexError(String),
}

pub type Result<T> = std::result::Result<T, VectorError>;