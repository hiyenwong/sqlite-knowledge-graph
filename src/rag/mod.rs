mod error;
pub use error::RagError;

use crate::graph::Entity;
use crate::vector::SearchResult;

/// RAG search result combining vector and graph information
#[derive(Debug, Clone)]
pub struct RagResult {
    pub entity: Entity,
    pub vector_score: f64,
    pub graph_score: f64,
    pub combined_score: f64,
    pub related_entities: Vec<Entity>,
}

/// RAG search configuration
pub struct RagConfig {
    pub vector_weight: f64,
    pub graph_weight: f64,
    pub neighbor_depth: i32,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.6,
            graph_weight: 0.4,
            neighbor_depth: 2,
        }
    }
}

/// Hybrid RAG engine
pub struct RagEngine {
    config: RagConfig,
}

impl RagEngine {
    pub fn new(config: RagConfig) -> Self {
        Self { config }
    }
    
    pub fn search(&self, _vector_results: Vec<SearchResult>, _k: usize) -> Vec<RagResult> {
        // TODO: Implement hybrid search
        // 1. Get vector search results
        // 2. Expand with graph neighbors
        // 3. Combine scores
        // 4. Rank and return top k
        todo!("RAG hybrid search not yet implemented")
    }
}