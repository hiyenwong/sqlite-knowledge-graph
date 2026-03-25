use crate::error::Result;
/// Louvain community detection algorithm
use rusqlite::Connection;
use std::collections::HashMap;

/// Community detection result
#[derive(Debug, Clone)]
pub struct CommunityResult {
    /// Entity to community mapping
    pub memberships: Vec<(i64, i32)>,
    /// Number of communities
    pub num_communities: i32,
    /// Modularity score
    pub modularity: f64,
}

/// Compute communities using Louvain algorithm
///
/// Returns community memberships and modularity score.
pub fn louvain_communities(conn: &Connection) -> Result<CommunityResult> {
    // Build adjacency list with weights
    let mut graph: HashMap<i64, HashMap<i64, f64>> = HashMap::new();
    let mut total_weight = 0.0;

    let mut stmt = conn.prepare("SELECT from_id, to_id, weight FROM relations")?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;

    for row in rows {
        let (from, to, weight) = row?;
        *graph.entry(from).or_default().entry(to).or_default() += weight;
        graph.entry(to).or_default(); // Ensure target node exists
        total_weight += weight;
    }

    if graph.is_empty() {
        return Ok(CommunityResult {
            memberships: Vec::new(),
            num_communities: 0,
            modularity: 0.0,
        });
    }

    let nodes: Vec<i64> = graph.keys().copied().collect();
    let _n = nodes.len();

    // Initialize: each node in its own community
    let mut community: HashMap<i64, i32> = nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i as i32))
        .collect();
    let mut improved = true;
    let mut iteration = 0;

    while improved && iteration < 100 {
        improved = false;
        iteration += 1;

        for &node in &nodes {
            let current_community = community[&node];

            // Find neighboring communities
            let neighbors: Vec<i64> = graph
                .get(&node)
                .map(|edges| edges.keys().copied().collect())
                .unwrap_or_default();

            let mut best_community = current_community;
            let mut best_gain = 0.0;

            for &neighbor in &neighbors {
                let neighbor_community = community[&neighbor];
                if neighbor_community == current_community {
                    continue;
                }

                // Calculate modularity gain (simplified)
                let gain = calculate_modularity_gain(
                    &graph,
                    node,
                    neighbor_community,
                    &community,
                    total_weight,
                );

                if gain > best_gain {
                    best_gain = gain;
                    best_community = neighbor_community;
                }
            }

            if best_community != current_community {
                community.insert(node, best_community);
                improved = true;
            }
        }
    }

    // Renumber communities consecutively
    let mut community_map: HashMap<i32, i32> = HashMap::new();
    let mut next_id = 0i32;

    for &comm in community.values() {
        if let std::collections::hash_map::Entry::Vacant(e) = community_map.entry(comm) {
            e.insert(next_id);
            next_id += 1;
        }
    }

    let memberships: Vec<(i64, i32)> = nodes
        .iter()
        .map(|&id| (id, community_map[&community[&id]]))
        .collect();

    // Calculate final modularity
    let modularity = calculate_modularity(&graph, &community, total_weight);

    Ok(CommunityResult {
        memberships,
        num_communities: next_id,
        modularity,
    })
}

fn calculate_modularity_gain(
    graph: &HashMap<i64, HashMap<i64, f64>>,
    node: i64,
    target_community: i32,
    community: &HashMap<i64, i32>,
    total_weight: f64,
) -> f64 {
    let mut gain = 0.0;

    if let Some(neighbors) = graph.get(&node) {
        for (&neighbor, &weight) in neighbors {
            if community.get(&neighbor) == Some(&target_community) {
                gain += weight / total_weight;
            }
        }
    }

    gain
}

fn calculate_modularity(
    graph: &HashMap<i64, HashMap<i64, f64>>,
    community: &HashMap<i64, i32>,
    total_weight: f64,
) -> f64 {
    if total_weight == 0.0 {
        return 0.0;
    }

    let mut modularity = 0.0;

    for (&from, edges) in graph {
        for (&to, &weight) in edges {
            if community.get(&from) == community.get(&to) {
                modularity += weight / total_weight;
            }
        }
    }

    modularity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE entities (id INTEGER PRIMARY KEY);
             CREATE TABLE relations (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, relation_type TEXT, weight REAL);"
        ).unwrap();

        // Create two communities: 1-2-3 and 4-5-6, with weak link between
        conn.execute(
            "INSERT INTO entities (id) VALUES (1), (2), (3), (4), (5), (6)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (1, 2, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (2, 3, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (4, 5, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (5, 6, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (3, 4, 'link', 0.1)", []).unwrap();

        conn
    }

    #[test]
    fn test_louvain() {
        let conn = setup_test_db();
        let result = louvain_communities(&conn).unwrap();

        assert!(result.num_communities >= 1);
        assert!(result.memberships.len() == 6);
    }

    #[test]
    fn test_empty_graph() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE entities (id INTEGER PRIMARY KEY); CREATE TABLE relations (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, relation_type TEXT, weight REAL);").unwrap();

        let result = louvain_communities(&conn).unwrap();
        assert_eq!(result.num_communities, 0);
    }
}
