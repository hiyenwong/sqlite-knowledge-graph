pub use crate::graph::GraphError;
pub use crate::vector::VectorError;
pub use crate::rag::RagError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Graph error: {0}")]
    Graph(#[from] GraphError),
    
    #[error("Vector error: {0}")]
    Vector(#[from] VectorError),
    
    #[error("RAG error: {0}")]
    Rag(#[from] RagError),
}

pub type Result<T> = std::result::Result<T, Error>;