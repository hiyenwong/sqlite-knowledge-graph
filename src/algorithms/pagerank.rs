use crate::error::Result;
/// PageRank algorithm implementation
use rusqlite::Connection;
use std::collections::HashMap;

/// PageRank configuration
#[derive(Debug, Clone)]
pub struct PageRankConfig {
    /// Damping factor (typically 0.85)
    pub damping: f64,
    /// Maximum iterations
    pub max_iterations: usize,
    /// Convergence threshold
    pub tolerance: f64,
}

impl Default for PageRankConfig {
    fn default() -> Self {
        Self {
            damping: 0.85,
            max_iterations: 100,
            tolerance: 1e-6,
        }
    }
}

/// Compute PageRank scores for all entities
///
/// Returns a vector of (entity_id, score) sorted by score descending.
pub fn pagerank(conn: &Connection, config: PageRankConfig) -> Result<Vec<(i64, f64)>> {
    // Build adjacency list from relations
    let mut out_edges: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut in_edges: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut all_nodes: HashSet<i64> = HashSet::new();

    let mut stmt = conn.prepare("SELECT source_id, target_id FROM kg_relations")?;

    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;

    for row in rows {
        let (from, to) = row?;
        all_nodes.insert(from);
        all_nodes.insert(to);
        out_edges.entry(from).or_default().push(to);
        in_edges.entry(to).or_default().push(from);
    }

    if all_nodes.is_empty() {
        return Ok(Vec::new());
    }

    let n = all_nodes.len() as f64;
    let initial_score = 1.0 / n;

    // Initialize scores
    let mut scores: HashMap<i64, f64> = all_nodes.iter().map(|&id| (id, initial_score)).collect();
    let mut new_scores: HashMap<i64, f64> = HashMap::new();

    // Iterate until convergence
    for _ in 0..config.max_iterations {
        let dangling_sum: f64 = all_nodes
            .iter()
            .filter(|&&id| match out_edges.get(&id) {
                None => true,
                Some(edges) => edges.is_empty(),
            })
            .map(|&id| scores[&id])
            .sum();

        for &node in &all_nodes {
            let incoming_score: f64 = in_edges
                .get(&node)
                .map(|edges| {
                    edges
                        .iter()
                        .map(|&from| {
                            let out_degree = out_edges.get(&from).map_or(1, |e| e.len()) as f64;
                            scores[&from] / out_degree
                        })
                        .sum()
                })
                .unwrap_or(0.0);

            new_scores.insert(
                node,
                (1.0 - config.damping) / n + config.damping * (incoming_score + dangling_sum / n),
            );
        }

        // Check convergence
        let diff: f64 = all_nodes
            .iter()
            .map(|&id| (scores[&id] - new_scores[&id]).abs())
            .sum();

        std::mem::swap(&mut scores, &mut new_scores);

        if diff < config.tolerance {
            break;
        }
    }

    // Sort by score descending
    let mut result: Vec<(i64, f64)> = scores.into_iter().collect();
    result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(result)
}

use std::collections::HashSet;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        // Create a simple graph: 1 -> 2 -> 3, 1 -> 3
        use crate::graph::entity::{insert_entity, Entity};
        use crate::graph::relation::{insert_relation, Relation};
        let id1 = insert_entity(&conn, &Entity::new("node", "Node 1")).unwrap();
        let id2 = insert_entity(&conn, &Entity::new("node", "Node 2")).unwrap();
        let id3 = insert_entity(&conn, &Entity::new("node", "Node 3")).unwrap();
        let _id4 = insert_entity(&conn, &Entity::new("node", "Node 4")).unwrap();
        insert_relation(&conn, &Relation::new(id1, id2, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id2, id3, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id1, id3, "link", 1.0).unwrap()).unwrap();

        conn
    }

    #[test]
    fn test_pagerank() {
        let conn = setup_test_db();
        let result = pagerank(&conn, PageRankConfig::default()).unwrap();

        // Only nodes with relations are included (3 of the 4 entities)
        assert_eq!(result.len(), 3);
        // Results are sorted by score descending; just verify we got scores
        assert!(result[0].1 >= result[1].1);
        assert!(result[1].1 >= result[2].1);
    }

    #[test]
    fn test_pagerank_empty_graph() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let result = pagerank(&conn, PageRankConfig::default()).unwrap();
        assert!(result.is_empty());
    }
}
