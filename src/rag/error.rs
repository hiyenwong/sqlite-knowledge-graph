use thiserror::Error;

#[derive(Error, Debug)]
pub enum RagError {
    #[error("No results found")]
    NoResults,

    #[error("Embedding failed: {0}")]
    Embedding(String),
}
