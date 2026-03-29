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

/// Compute communities using a simplified single-phase Louvain algorithm.
///
/// This implements Phase 1 of the Louvain method (local node moves) only.
/// Phase 2 (community aggregation into super-nodes) is not implemented.
/// For graphs with deep hierarchical community structure, results may be
/// sub-optimal compared to the full two-phase algorithm.
///
/// Returns community memberships and modularity score.
pub fn louvain_communities(conn: &Connection) -> Result<CommunityResult> {
    // Build adjacency list with weights
    let mut graph: HashMap<i64, HashMap<i64, f64>> = HashMap::new();
    let mut total_weight = 0.0;

    let mut stmt = conn.prepare("SELECT source_id, target_id, weight FROM kg_relations")?;

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
    if total_weight == 0.0 {
        return 0.0;
    }

    let m = total_weight;

    // k_i: degree (sum of weights) of the node being moved
    let k_i: f64 = graph
        .get(&node)
        .map(|edges| edges.values().sum())
        .unwrap_or(0.0);

    // k_i_in: sum of weights from node to nodes already in target_community
    let k_i_in: f64 = graph
        .get(&node)
        .map(|edges| {
            edges
                .iter()
                .filter(|(&nbr, _)| community.get(&nbr) == Some(&target_community))
                .map(|(_, &w)| w)
                .sum()
        })
        .unwrap_or(0.0);

    // k_tot: sum of degrees of all nodes in target_community
    let k_tot: f64 = graph
        .iter()
        .filter(|(&id, _)| id != node && community.get(&id) == Some(&target_community))
        .map(|(_, edges)| edges.values().sum::<f64>())
        .sum();

    // Standard Louvain ΔQ = k_i_in / m  -  k_tot * k_i / (2 * m²)
    k_i_in / m - k_tot * k_i / (2.0 * m * m)
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
        crate::schema::create_schema(&conn).unwrap();

        // Create two communities: 1-2-3 and 4-5-6, with weak link between
        use crate::graph::entity::{insert_entity, Entity};
        use crate::graph::relation::{insert_relation, Relation};
        let id1 = insert_entity(&conn, &Entity::new("node", "Node 1")).unwrap();
        let id2 = insert_entity(&conn, &Entity::new("node", "Node 2")).unwrap();
        let id3 = insert_entity(&conn, &Entity::new("node", "Node 3")).unwrap();
        let id4 = insert_entity(&conn, &Entity::new("node", "Node 4")).unwrap();
        let id5 = insert_entity(&conn, &Entity::new("node", "Node 5")).unwrap();
        let id6 = insert_entity(&conn, &Entity::new("node", "Node 6")).unwrap();
        insert_relation(&conn, &Relation::new(id1, id2, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id2, id3, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id4, id5, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id5, id6, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id3, id4, "link", 0.1).unwrap()).unwrap();

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
        crate::schema::create_schema(&conn).unwrap();

        let result = louvain_communities(&conn).unwrap();
        assert_eq!(result.num_communities, 0);
    }
}
