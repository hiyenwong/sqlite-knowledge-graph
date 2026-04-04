use crate::error::Result;
/// Louvain community detection algorithm (full two-phase)
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

/// Compute communities using the full two-phase Louvain algorithm.
///
/// - Phase 1: each node greedily moves to the adjacent community that
///   maximises modularity gain (local optimisation).
/// - Phase 2: all communities are collapsed into super-nodes; the graph is
///   rebuilt with aggregated edge weights and Phase 1 is repeated.
///
/// The two phases alternate until no further improvement is possible.
pub fn louvain_communities(conn: &Connection) -> Result<CommunityResult> {
    // ── Build initial weighted graph from DB ──────────────────────────────
    let mut init_graph: HashMap<i64, HashMap<i64, f64>> = HashMap::new();
    let mut total_weight = 0.0;

    let mut stmt = conn.prepare("SELECT source_id, target_id, weight FROM kg_relations")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            crate::row_get_weight(row, 2)?,
        ))
    })?;

    for row in rows {
        let (from, to, weight) = row?;
        *init_graph.entry(from).or_default().entry(to).or_default() += weight;
        init_graph.entry(to).or_default(); // ensure target node exists
        total_weight += weight;
    }

    if init_graph.is_empty() {
        return Ok(CommunityResult {
            memberships: Vec::new(),
            num_communities: 0,
            modularity: 0.0,
        });
    }

    // Stable ordering so tests are deterministic
    let orig_nodes: Vec<i64> = {
        let mut v: Vec<i64> = init_graph.keys().copied().collect();
        v.sort_unstable();
        v
    };
    let n = orig_nodes.len();
    let id_to_idx: HashMap<i64, usize> = orig_nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    // Convert to usize-keyed graph (work_graph nodes = 0..n-1 initially)
    let mut work_graph: HashMap<usize, HashMap<usize, f64>> = HashMap::new();
    for (&from, edges) in &init_graph {
        let fi = id_to_idx[&from];
        work_graph.entry(fi).or_default();
        for (&to, &w) in edges {
            let ti = id_to_idx[&to];
            *work_graph.entry(fi).or_default().entry(ti).or_default() += w;
        }
    }

    // orig_community[i] = final community index for original node i
    let mut orig_community: Vec<usize> = (0..n).collect();

    // sn_members[sn] = original node indices belonging to super-node sn
    let mut sn_members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();

    // ── Alternating Phase 1 / Phase 2 ─────────────────────────────────────
    loop {
        let m = sn_members.len(); // current number of super-nodes / work-nodes

        // Phase 1 ── greedy local moves
        // Initial assignment: each super-node in its own community
        let mut community: Vec<usize> = (0..m).collect();
        let work_nodes: Vec<usize> = (0..m).collect();

        let mut any_improved = false;
        let mut phase_improved = true;
        let mut iter = 0;

        while phase_improved && iter < 100 {
            phase_improved = false;
            iter += 1;

            for &node in &work_nodes {
                let cur_comm = community[node];

                let neighbors: Vec<usize> = work_graph
                    .get(&node)
                    .map(|e| e.keys().copied().collect())
                    .unwrap_or_default();

                let mut best_comm = cur_comm;
                let mut best_gain = 0.0_f64;

                for &nbr in &neighbors {
                    let nbr_comm = community[nbr];
                    if nbr_comm == cur_comm {
                        continue;
                    }

                    let gain =
                        modularity_gain(&work_graph, node, nbr_comm, &community, total_weight);
                    if gain > best_gain {
                        best_gain = gain;
                        best_comm = nbr_comm;
                    }
                }

                if best_comm != cur_comm {
                    community[node] = best_comm;
                    phase_improved = true;
                    any_improved = true;
                }
            }
        }

        if !any_improved {
            break; // converged globally
        }

        // Renumber communities to 0..num_new-1
        let mut unique_comms: Vec<usize> = community.clone();
        unique_comms.sort_unstable();
        unique_comms.dedup();
        let num_new = unique_comms.len();

        // comm_remap[old_community_id] = new_community_id
        // All community IDs are in 0..m-1, so a Vec of size m works.
        let mut comm_remap = vec![0usize; m];
        for (new_id, &old_comm) in unique_comms.iter().enumerate() {
            comm_remap[old_comm] = new_id;
        }

        // Propagate community assignments back to original nodes
        for (sn, members) in sn_members.iter().enumerate() {
            let new_comm = comm_remap[community[sn]];
            for &orig in members {
                orig_community[orig] = new_comm;
            }
        }

        if num_new == m {
            // Phase 1 didn't reduce the number of communities → done
            break;
        }

        // Phase 2 ── aggregate into super-nodes
        let mut new_sn_members: Vec<Vec<usize>> = vec![Vec::new(); num_new];
        for (sn, members) in sn_members.iter().enumerate() {
            let new_sn = comm_remap[community[sn]];
            new_sn_members[new_sn].extend_from_slice(members);
        }

        let mut new_graph: HashMap<usize, HashMap<usize, f64>> =
            (0..num_new).map(|i| (i, HashMap::new())).collect();
        for (&from_sn, edges) in &work_graph {
            let from_new = comm_remap[community[from_sn]];
            for (&to_sn, &weight) in edges {
                let to_new = comm_remap[community[to_sn]];
                // Self-loops are included; they affect total degree but not ΔQ.
                *new_graph
                    .entry(from_new)
                    .or_default()
                    .entry(to_new)
                    .or_default() += weight;
            }
        }

        work_graph = new_graph;
        sn_members = new_sn_members;
    }

    // ── Build final result ─────────────────────────────────────────────────
    // Assign consecutive i32 community IDs for the public API
    let mut comm_to_final: HashMap<usize, i32> = HashMap::new();
    let mut next_id = 0i32;

    let memberships: Vec<(i64, i32)> = orig_nodes
        .iter()
        .enumerate()
        .map(|(i, &entity_id)| {
            let comm = orig_community[i];
            let final_comm = *comm_to_final.entry(comm).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
            (entity_id, final_comm)
        })
        .collect();

    let num_communities = next_id;

    // Compute modularity on the original (unaggregated) graph
    let final_comm_map: HashMap<i64, usize> = orig_nodes
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, orig_community[i]))
        .collect();
    let modularity = compute_modularity(&init_graph, &final_comm_map, total_weight);

    Ok(CommunityResult {
        memberships,
        num_communities,
        modularity,
    })
}

/// Louvain modularity gain ΔQ for moving `node` into `target_community`.
///
/// ΔQ = k_{i,in} / m  −  Σ_tot · k_i / (2m²)
///
/// where m = total edge weight, k_i = degree of node,
/// k_{i,in} = weights from node to nodes already in target_community,
/// Σ_tot = sum of degrees of all nodes currently in target_community.
fn modularity_gain(
    graph: &HashMap<usize, HashMap<usize, f64>>,
    node: usize,
    target_community: usize,
    community: &[usize],
    total_weight: f64,
) -> f64 {
    if total_weight == 0.0 {
        return 0.0;
    }
    let m = total_weight;

    let k_i: f64 = graph
        .get(&node)
        .map(|edges| edges.values().sum())
        .unwrap_or(0.0);

    let k_i_in: f64 = graph
        .get(&node)
        .map(|edges| {
            edges
                .iter()
                .filter(|(&nbr, _)| community[nbr] == target_community)
                .map(|(_, &w)| w)
                .sum()
        })
        .unwrap_or(0.0);

    // Sum of degrees of all nodes *already* in target_community (excluding `node`)
    let k_tot: f64 = graph
        .iter()
        .filter(|(&id, _)| id != node && community[id] == target_community)
        .map(|(_, edges)| edges.values().sum::<f64>())
        .sum();

    k_i_in / m - k_tot * k_i / (2.0 * m * m)
}

/// Compute modularity Q on the original graph given a final community assignment.
fn compute_modularity(
    graph: &HashMap<i64, HashMap<i64, f64>>,
    community: &HashMap<i64, usize>,
    total_weight: f64,
) -> f64 {
    if total_weight == 0.0 {
        return 0.0;
    }
    let mut q = 0.0;
    for (&from, edges) in graph {
        for (&to, &weight) in edges {
            if community.get(&from) == community.get(&to) {
                q += weight / total_weight;
            }
        }
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        // Two communities: 1-2-3 and 4-5-6, with a weak cross-link 3→4
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
        assert_eq!(result.memberships.len(), 6);
        // With strong intra-cluster edges and a weak cross-link, we expect 2 communities
        assert!(result.num_communities <= 2);
    }

    #[test]
    fn test_empty_graph() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let result = louvain_communities(&conn).unwrap();
        assert_eq!(result.num_communities, 0);
    }

    #[test]
    fn test_single_community() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        // Fully connected triangle — should form one community
        use crate::graph::entity::{insert_entity, Entity};
        use crate::graph::relation::{insert_relation, Relation};
        let id1 = insert_entity(&conn, &Entity::new("node", "A")).unwrap();
        let id2 = insert_entity(&conn, &Entity::new("node", "B")).unwrap();
        let id3 = insert_entity(&conn, &Entity::new("node", "C")).unwrap();
        insert_relation(&conn, &Relation::new(id1, id2, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id2, id3, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id1, id3, "link", 1.0).unwrap()).unwrap();

        let result = louvain_communities(&conn).unwrap();
        assert_eq!(result.memberships.len(), 3);
        assert!(result.num_communities >= 1);
    }
}
