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

    let mut stmt = conn.prepare("SELECT from_id, to_id FROM relations")?;

    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;

    for row in rows {
        let (from, to) = row?;
        all_nodes.insert(from);
        all_nodes.insert(to);
        graph.entry(from).or_default().push(to);
        graph.entry(to).or_default().push(from);
    }

    // Add isolated nodes
    let mut stmt = conn.prepare("SELECT id FROM entities")?;
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

    let mut stmt = conn.prepare("SELECT from_id, to_id FROM relations")?;
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

    // First pass: order by finish time
    let mut visited = HashSet::new();
    let mut finish_order = Vec::new();

    fn dfs1(
        node: i64,
        graph: &HashMap<i64, Vec<i64>>,
        visited: &mut HashSet<i64>,
        finish_order: &mut Vec<i64>,
    ) {
        visited.insert(node);
        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    dfs1(neighbor, graph, visited, finish_order);
                }
            }
        }
        finish_order.push(node);
    }

    for &node in &all_nodes {
        if !visited.contains(&node) {
            dfs1(node, &graph, &mut visited, &mut finish_order);
        }
    }

    // Second pass: collect SCCs
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    fn dfs2(
        node: i64,
        reverse_graph: &HashMap<i64, Vec<i64>>,
        visited: &mut HashSet<i64>,
        component: &mut Vec<i64>,
    ) {
        visited.insert(node);
        component.push(node);
        if let Some(neighbors) = reverse_graph.get(&node) {
            for &neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    dfs2(neighbor, reverse_graph, visited, component);
                }
            }
        }
    }

    for &node in finish_order.iter().rev() {
        if !visited.contains(&node) {
            let mut component = Vec::new();
            dfs2(node, &reverse_graph, &mut visited, &mut component);
            components.push(component);
        }
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
        conn.execute_batch(
            "CREATE TABLE entities (id INTEGER PRIMARY KEY);
             CREATE TABLE relations (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, relation_type TEXT, weight REAL);"
        ).unwrap();

        // Create two disconnected components: 1-2-3 and 4-5
        conn.execute(
            "INSERT INTO entities (id) VALUES (1), (2), (3), (4), (5)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (1, 2, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (2, 3, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (4, 5, 'link', 1.0)", []).unwrap();

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
        conn.execute_batch(
            "CREATE TABLE entities (id INTEGER PRIMARY KEY);
             CREATE TABLE relations (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, relation_type TEXT, weight REAL);"
        ).unwrap();

        // Create a cycle: 1 -> 2 -> 3 -> 1
        conn.execute("INSERT INTO entities (id) VALUES (1), (2), (3)", [])
            .unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (1, 2, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (2, 3, 'link', 1.0)", []).unwrap();
        conn.execute("INSERT INTO relations (from_id, to_id, relation_type, weight) VALUES (3, 1, 'link', 1.0)", []).unwrap();

        let components = strongly_connected_components(&conn).unwrap();

        // All three nodes should be in one SCC
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].len(), 3);
    }
}
