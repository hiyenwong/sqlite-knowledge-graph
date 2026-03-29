use crate::error::Result;
/// Graph traversal algorithms for sqlite-knowledge-graph
///
/// Provides BFS/DFS traversal, shortest path, and graph statistics.
use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};

/// Node with depth information for traversal results
#[derive(Debug, Clone)]
pub struct TraversalNode {
    pub entity_id: i64,
    pub entity_type: String,
    pub depth: u32,
}

/// Step in a path (edge + target node)
#[derive(Debug, Clone)]
pub struct PathStep {
    pub from_id: i64,
    pub to_id: i64,
    pub relation_type: String,
    pub weight: f64,
}

/// Complete path from source to target
#[derive(Debug, Clone)]
pub struct TraversalPath {
    pub start_id: i64,
    pub end_id: i64,
    pub steps: Vec<PathStep>,
    pub total_weight: f64,
}

/// Graph statistics
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_entities: i64,
    pub total_relations: i64,
    pub avg_degree: f64,
    pub max_degree: i64,
    pub density: f64,
}

/// Direction for traversal
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

/// Query parameters for traversal
#[derive(Debug, Clone)]
pub struct TraversalQuery {
    pub direction: Direction,
    pub rel_types: Option<Vec<String>>,
    pub min_weight: Option<f64>,
    pub max_depth: u32,
}

impl Default for TraversalQuery {
    fn default() -> Self {
        Self {
            direction: Direction::Both,
            rel_types: None,
            min_weight: None,
            max_depth: 3,
        }
    }
}

/// BFS traversal from a starting entity
///
/// Returns all reachable entities within max_depth, with their depth information.
pub fn bfs_traversal(
    conn: &Connection,
    start_id: i64,
    query: TraversalQuery,
) -> Result<Vec<TraversalNode>> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Get start entity type
    let start_type: String = conn.query_row(
        "SELECT entity_type FROM kg_entities WHERE id = ?1",
        [start_id],
        |row| row.get(0),
    )?;

    queue.push_back((start_id, start_type, 0u32));
    visited.insert(start_id);

    while let Some((entity_id, _entity_type, depth)) = queue.pop_front() {
        if depth > query.max_depth {
            continue;
        }

        result.push(TraversalNode {
            entity_id,
            entity_type: _entity_type.clone(),
            depth,
        });

        if depth == query.max_depth {
            continue;
        }

        // Get neighbors based on direction
        let neighbors = get_neighbors(conn, entity_id, &query)?;

        for (neighbor_id, neighbor_type) in neighbors {
            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id);
                queue.push_back((neighbor_id, neighbor_type, depth + 1));
            }
        }
    }

    Ok(result)
}

/// DFS traversal from a starting entity
///
/// Returns all reachable entities within max_depth using depth-first search.
pub fn dfs_traversal(
    conn: &Connection,
    start_id: i64,
    query: TraversalQuery,
) -> Result<Vec<TraversalNode>> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();

    // Get start entity type
    let start_type: String = conn.query_row(
        "SELECT entity_type FROM kg_entities WHERE id = ?1",
        [start_id],
        |row| row.get(0),
    )?;

    dfs_visit(
        conn,
        start_id,
        start_type,
        0,
        &query,
        &mut visited,
        &mut result,
    )?;

    Ok(result)
}

fn dfs_visit(
    conn: &Connection,
    entity_id: i64,
    entity_type: String,
    depth: u32,
    query: &TraversalQuery,
    visited: &mut HashSet<i64>,
    result: &mut Vec<TraversalNode>,
) -> Result<()> {
    if visited.contains(&entity_id) || depth > query.max_depth {
        return Ok(());
    }

    visited.insert(entity_id);
    result.push(TraversalNode {
        entity_id,
        entity_type: entity_type.clone(),
        depth,
    });

    if depth == query.max_depth {
        return Ok(());
    }

    let neighbors = get_neighbors(conn, entity_id, query)?;

    for (neighbor_id, neighbor_type) in neighbors {
        dfs_visit(
            conn,
            neighbor_id,
            neighbor_type,
            depth + 1,
            query,
            visited,
            result,
        )?;
    }

    Ok(())
}

/// Find shortest path between two entities using BFS
///
/// Returns the shortest path (if exists) with all intermediate steps.
pub fn find_shortest_path(
    conn: &Connection,
    from_id: i64,
    to_id: i64,
    max_depth: u32,
) -> Result<Option<TraversalPath>> {
    if from_id == to_id {
        return Ok(Some(TraversalPath {
            start_id: from_id,
            end_id: to_id,
            steps: Vec::new(),
            total_weight: 0.0,
        }));
    }

    let mut visited = HashMap::new(); // entity_id -> (from_id, relation_type, weight)
    let mut queue: VecDeque<(i64, u32)> = VecDeque::new(); // (entity_id, depth)

    queue.push_back((from_id, 0));
    visited.insert(from_id, None);

    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= max_depth {
            continue;
        }

        // Get outgoing relations
        let relations = get_outgoing_relations(conn, current_id)?;

        for (target_id, rel_type, weight) in relations {
            if let std::collections::hash_map::Entry::Vacant(e) = visited.entry(target_id) {
                e.insert(Some((current_id, rel_type.clone(), weight)));

                if target_id == to_id {
                    // Reconstruct path
                    return Ok(Some(reconstruct_path(from_id, to_id, &visited)?));
                }

                queue.push_back((target_id, current_depth + 1));
            }
        }
    }

    Ok(None)
}

/// Compute graph statistics
pub fn compute_graph_stats(conn: &Connection) -> Result<GraphStats> {
    let total_entities: i64 =
        conn.query_row("SELECT COUNT(*) FROM kg_entities", [], |row| row.get(0))?;

    let total_relations: i64 =
        conn.query_row("SELECT COUNT(*) FROM kg_relations", [], |row| row.get(0))?;

    let max_degree: i64 = conn.query_row(
        "SELECT COALESCE(MAX(cnt), 0) FROM (
            SELECT source_id as id, COUNT(*) as cnt FROM kg_relations GROUP BY source_id
            UNION ALL
            SELECT target_id as id, COUNT(*) as cnt FROM kg_relations GROUP BY target_id
        )",
        [],
        |row| row.get(0),
    )?;

    let avg_degree = if total_entities > 0 {
        (total_relations as f64 * 2.0) / (total_entities as f64)
    } else {
        0.0
    };

    let density = if total_entities > 1 {
        let possible_edges = total_entities * (total_entities - 1);
        total_relations as f64 / possible_edges as f64
    } else {
        0.0
    };

    Ok(GraphStats {
        total_entities,
        total_relations,
        avg_degree,
        max_degree,
        density,
    })
}

// Helper functions

fn get_neighbors(
    conn: &Connection,
    entity_id: i64,
    query: &TraversalQuery,
) -> Result<Vec<(i64, String)>> {
    let mut neighbors = Vec::new();

    let sql = match query.direction {
        Direction::Outgoing => {
            "SELECT r.target_id, e.entity_type FROM kg_relations r
             JOIN kg_entities e ON r.target_id = e.id
             WHERE r.source_id = ?1"
        }
        Direction::Incoming => {
            "SELECT r.source_id, e.entity_type FROM kg_relations r
             JOIN kg_entities e ON r.source_id = e.id
             WHERE r.target_id = ?1"
        }
        Direction::Both => {
            "SELECT r.target_id, e.entity_type FROM kg_relations r
             JOIN kg_entities e ON r.target_id = e.id
             WHERE r.source_id = ?1
             UNION
             SELECT r.source_id, e.entity_type FROM kg_relations r
             JOIN kg_entities e ON r.source_id = e.id
             WHERE r.target_id = ?1"
        }
    };

    let mut stmt = conn.prepare(sql)?;

    let rows = stmt.query_map([entity_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (id, entity_type) = row?;
        neighbors.push((id, entity_type));
    }

    Ok(neighbors)
}

fn get_outgoing_relations(conn: &Connection, entity_id: i64) -> Result<Vec<(i64, String, f64)>> {
    let mut relations = Vec::new();

    let mut stmt =
        conn.prepare("SELECT target_id, rel_type, weight FROM kg_relations WHERE source_id = ?1")?;

    let rows = stmt.query_map([entity_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;

    for row in rows {
        relations.push(row?);
    }

    Ok(relations)
}

fn reconstruct_path(
    from_id: i64,
    to_id: i64,
    visited: &HashMap<i64, Option<(i64, String, f64)>>,
) -> Result<TraversalPath> {
    let mut steps = Vec::new();
    let mut current = to_id;
    let mut total_weight = 0.0;

    while current != from_id {
        if let Some(Some((from, rel_type, weight))) = visited.get(&current) {
            steps.push(PathStep {
                from_id: *from,
                to_id: current,
                relation_type: rel_type.clone(),
                weight: *weight,
            });
            total_weight += weight;
            current = *from;
        } else {
            break;
        }
    }

    steps.reverse();

    Ok(TraversalPath {
        start_id: from_id,
        end_id: to_id,
        steps,
        total_weight,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        use crate::graph::entity::{insert_entity, Entity};
        use crate::graph::relation::{insert_relation, Relation};

        // Insert test entities: A=1, B=2, C=3, D=4
        let id_a = insert_entity(&conn, &Entity::new("paper", "A")).unwrap();
        let id_b = insert_entity(&conn, &Entity::new("paper", "B")).unwrap();
        let id_c = insert_entity(&conn, &Entity::new("paper", "C")).unwrap();
        let id_d = insert_entity(&conn, &Entity::new("paper", "D")).unwrap();

        // Insert test relations: A -> B -> C, A -> D
        insert_relation(&conn, &Relation::new(id_a, id_b, "cites", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id_b, id_c, "cites", 1.0).unwrap()).unwrap();
        insert_relation(&conn, &Relation::new(id_a, id_d, "cites", 0.5).unwrap()).unwrap();

        conn
    }

    #[test]
    fn test_bfs_traversal() {
        let conn = setup_test_db();
        let query = TraversalQuery {
            direction: Direction::Outgoing,
            max_depth: 2,
            ..Default::default()
        };

        let result = bfs_traversal(&conn, 1, query).unwrap();

        assert_eq!(result.len(), 4); // A, B, C, D
        assert!(result.iter().any(|n| n.entity_id == 1 && n.depth == 0));
        assert!(result.iter().any(|n| n.entity_id == 2 && n.depth == 1));
        assert!(result.iter().any(|n| n.entity_id == 3 && n.depth == 2));
        assert!(result.iter().any(|n| n.entity_id == 4 && n.depth == 1));
    }

    #[test]
    fn test_dfs_traversal() {
        let conn = setup_test_db();
        let query = TraversalQuery {
            direction: Direction::Outgoing,
            max_depth: 2,
            ..Default::default()
        };

        let result = dfs_traversal(&conn, 1, query).unwrap();

        assert_eq!(result.len(), 4);
        assert_eq!(result[0].entity_id, 1); // DFS visits start first
    }

    #[test]
    fn test_shortest_path() {
        let conn = setup_test_db();

        // Path A -> B -> C
        let path = find_shortest_path(&conn, 1, 3, 5).unwrap();
        assert!(path.is_some());

        let path = path.unwrap();
        assert_eq!(path.start_id, 1);
        assert_eq!(path.end_id, 3);
        assert_eq!(path.steps.len(), 2); // A->B, B->C

        // Direct path A -> D
        let path = find_shortest_path(&conn, 1, 4, 5).unwrap();
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.steps.len(), 1);
    }

    #[test]
    fn test_no_path() {
        let conn = setup_test_db();

        // No path from D to A
        let path = find_shortest_path(&conn, 4, 1, 5).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn test_graph_stats() {
        let conn = setup_test_db();

        let stats = compute_graph_stats(&conn).unwrap();

        assert_eq!(stats.total_entities, 4);
        assert_eq!(stats.total_relations, 3);
        assert_eq!(stats.max_degree, 2); // Entity 1 has 2 outgoing edges
    }
}
