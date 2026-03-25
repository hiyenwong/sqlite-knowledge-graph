//! Vector module for semantic search with cosine similarity and TurboQuant indexing.

pub mod store;
pub mod turboquant;

pub use store::{cosine_similarity, SearchResult, VectorStore};
pub use turboquant::{TurboQuantConfig, TurboQuantIndex, TurboQuantStats};
