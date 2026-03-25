/// Graph algorithms for sqlite-knowledge-graph
///
/// Provides PageRank, Louvain community detection, and connected components.

use rusqlite::Connection;
use crate::error::Result;

mod pagerank;
mod louvain;
mod connected;

pub use pagerank::{pagerank, PageRankConfig};
pub use louvain::{louvain_communities, CommunityResult};
pub use connected::{connected_components, strongly_connected_components};

/// Run all graph algorithms and return summary
pub fn analyze_graph(conn: &Connection) -> Result<GraphAnalysis> {
    let pr = pagerank(conn, PageRankConfig::default())?;
    let communities = louvain_communities(conn)?;
    let components = connected_components(conn)?;

    Ok(GraphAnalysis {
        pagerank: pr,
        communities,
        num_components: components.len(),
        largest_component_size: components.iter().map(|c| c.len()).max().unwrap_or(0),
    })
}

#[derive(Debug, Clone)]
pub struct GraphAnalysis {
    pub pagerank: Vec<(i64, f64)>,
    pub communities: CommunityResult,
    pub num_components: usize,
    pub largest_component_size: usize,
}