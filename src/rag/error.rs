use thiserror::Error;

#[derive(Error, Debug)]
pub enum RagError {
    #[error("No results found")]
    NoResults,
    
    #[error("Graph error: {0}")]
    Graph(#[from] crate::graph::GraphError),
    
    #[error("Vector error: {0}")]
    Vector(#[from] crate::vector::VectorError),
}

pub type Result<T> = std::result::Result<T, RagError>;