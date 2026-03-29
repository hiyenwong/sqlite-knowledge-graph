use crate::error::Result;
/// Connected components algorithms
use std::cmp::Reverse;

use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};

/// Find weakly connected components
///
/// Returns a list of components, each being a list of entity IDs.
pub fn connected_components(conn: &Connection) -> Result<Vec<Vec<i64>>> {
    // Build undirected adjacency list
    let mut graph: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut all_nodes: HashSet<i64> = HashSet::new();

    let mut stmt = conn.prepare("SELECT source_id, target_id FROM kg_relations")?;

    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;

    for row in rows {
        let (from, to) = row?;
        all_nodes.insert(from);
        all_nodes.insert(to);
        graph.entry(from).or_default().push(to);
        graph.entry(to).or_default().push(from);
    }

    // Add isolated nodes
    let mut stmt = conn.prepare("SELECT id FROM kg_entities")?;
    let entity_rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    for row in entity_rows {
        let id = row?;
        all_nodes.insert(id);
        graph.entry(id).or_default();
    }

    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for &start in &all_nodes {
        if visited.contains(&start) {
            continue;
        }

        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some(node) = queue.pop_front() {
            component.push(node);

            if let Some(neighbors) = graph.get(&node) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        components.push(component);
    }

    // Sort by size descending
    components.sort_by_key(|b| Reverse(b.len()));

    Ok(components)
}

/// Find strongly connected components using Kosaraju's algorithm
///
/// Returns a list of strongly connected components.
pub fn strongly_connected_components(conn: &Connection) -> Result<Vec<Vec<i64>>> {
    // Build directed adjacency list
    let mut graph: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut reverse_graph: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut all_nodes: HashSet<i64> = HashSet::new();

    let mut stmt = conn.prepare("SELECT source_id, target_id FROM kg_relations")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;

    for row in rows {
        let (from, to) = row?;
        all_nodes.insert(from);
        all_nodes.insert(to);
        graph.entry(from).or_default().push(to);
        reverse_graph.entry(to).or_default().push(from);
        graph.entry(to).or_default();
        reverse_graph.entry(from).or_default();
    }

    // First pass: compute finish order iteratively (avoids stack overflow on large chains)
    let mut visited = HashSet::new();
    let mut finish_order = Vec::new();

    for &start in &all_nodes {
        if visited.contains(&start) {
            continue;
        }
        // Iterative DFS with explicit stack; each entry is (node, iterator_index)
        let mut stack: Vec<(i64, usize)> = vec![(start, 0)];
        visited.insert(start);
        while let Some((node, idx)) = stack.last_mut() {
            let node = *node;
            let neighbors = graph.get(&node).map(|v| v.as_slice()).unwrap_or(&[]);
            if *idx < neighbors.len() {
                let neighbor = neighbors[*idx];
                *idx += 1;
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    stack.push((neighbor, 0));
                }
            } else {
                finish_order.push(node);
                stack.pop();
            }
        }
    }

    // Second pass: collect SCCs iteratively (reverse graph BFS/DFS)
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for &start in finish_order.iter().rev() {
        if visited.contains(&start) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        visited.insert(start);
        while let Some(node) = stack.pop() {
            component.push(node);
            if let Some(neighbors) = reverse_graph.get(&node) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        stack.push(neighbor);
                    }
                }
            }
        }
        components.push(component);
    }

    // Sort by size descending
    components.sort_by_key(|b| Reverse(b.len()));

    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        // Create two disconnected components: 1-2-3 and 4-5
        use crate::graph::entity::{insert_entity, Entity};
        use crate::graph::relation::{insert_relation, Relation};
        let id1 = insert_entity(&conn, &Entity::new("node", "Node 1")).unwrap();
        let id2 = insert_entity(&conn, &Entity::new("node", "Node 2")).unwrap();
        let id3 = insert_entity(&conn, &Entity::new("node", "Node 3")).unwrap();
        let id4 = insert_entity(&conn, &Entity::new("node", "Node 4")).unwrap();
        let id5 = insert_entity(&conn, &Entity::new("node", "Node 5")).unwrap();
        insert_relation(&conn, &Relation::new(id1, id2, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id2, id3, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id4, id5, "link", 1.0).unwrap()).unwrap();

        conn
    }

    #[test]
    fn test_connected_components() {
        let conn = setup_test_db();
        let components = connected_components(&conn).unwrap();

        assert_eq!(components.len(), 2);
        assert_eq!(components[0].len(), 3); // Largest component
        assert_eq!(components[1].len(), 2);
    }

    #[test]
    fn test_strongly_connected_components() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        // Create a cycle: 1 -> 2 -> 3 -> 1
        use crate::graph::entity::{insert_entity, Entity};
        use crate::graph::relation::{insert_relation, Relation};
        let id1 = insert_entity(&conn, &Entity::new("node", "Node 1")).unwrap();
        let id2 = insert_entity(&conn, &Entity::new("node", "Node 2")).unwrap();
        let id3 = insert_entity(&conn, &Entity::new("node", "Node 3")).unwrap();
        insert_relation(&conn, &Relation::new(id1, id2, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id2, id3, "link", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id3, id1, "link", 1.0).unwrap()).unwrap();

        let components = strongly_connected_components(&conn).unwrap();

        // All three nodes should be in one SCC
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].len(), 3);
    }
}
