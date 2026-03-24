//! Vector module for semantic search with cosine similarity.

pub mod store;

pub use store::{cosine_similarity, SearchResult, VectorStore};
