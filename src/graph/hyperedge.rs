//! Hyperedge (higher-order relation) storage module.
//!
//! Provides storage and querying for multi-entity relationships (hyperedges).
//! A hyperedge connects 2 or more entities in a single relation.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, Result};
use crate::graph::entity::{get_entity, Entity};

/// A hyperedge representing a higher-order relation among multiple entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hyperedge {
    pub id: Option<i64>,
    pub hyperedge_type: String,
    pub entity_ids: Vec<i64>,
    pub weight: f64,
    pub arity: usize,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

impl Hyperedge {
    /// Create a new hyperedge.
    ///
    /// Requires at least 2 entities and weight in [0.0, 1.0].
    pub fn new(
        entity_ids: Vec<i64>,
        hyperedge_type: impl Into<String>,
        weight: f64,
    ) -> Result<Self> {
        if entity_ids.len() < 2 {
            return Err(Error::InvalidArity(entity_ids.len()));
        }
        if !(0.0..=1.0).contains(&weight) {
            return Err(Error::InvalidWeight(weight));
        }

        let arity = entity_ids.len();
        Ok(Self {
            id: None,
            hyperedge_type: hyperedge_type.into(),
            entity_ids,
            weight,
            arity,
            properties: HashMap::new(),
            created_at: None,
            updated_at: None,
        })
    }

    /// Set a property on the hyperedge.
    pub fn set_property(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.properties.insert(key.into(), value);
    }

    /// Get a property value.
    pub fn get_property(&self, key: &str) -> Option<&serde_json::Value> {
        self.properties.get(key)
    }

    /// Check if an entity participates in this hyperedge.
    pub fn contains(&self, entity_id: i64) -> bool {
        self.entity_ids.contains(&entity_id)
    }

    /// Get the entity set for efficient set operations.
    pub fn entity_set(&self) -> HashSet<i64> {
        self.entity_ids.iter().copied().collect()
    }

    /// Compute intersection with another hyperedge - O(k1 + k2).
    pub fn intersection(&self, other: &Hyperedge) -> Vec<i64> {
        let set1 = self.entity_set();
        let set2 = other.entity_set();
        set1.intersection(&set2).copied().collect()
    }

    /// Check if this hyperedge shares any entity with another - O(k1 + k2).
    pub fn has_intersection(&self, other: &Hyperedge) -> bool {
        let set1 = self.entity_set();
        other.entity_ids.iter().any(|id| set1.contains(id))
    }
}

/// A higher-order neighbor: an entity connected through a hyperedge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HigherOrderNeighbor {
    pub entity: Entity,
    pub hyperedge: Hyperedge,
    pub position: Option<usize>,
}

/// A step in a higher-order path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HigherOrderPathStep {
    pub hyperedge: Hyperedge,
    pub from_entity: i64,
    pub to_entity: i64,
}

/// A higher-order path between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HigherOrderPath {
    pub steps: Vec<HigherOrderPathStep>,
    pub total_weight: f64,
}

/// Insert a hyperedge into the database.
pub fn insert_hyperedge(conn: &rusqlite::Connection, hyperedge: &Hyperedge) -> Result<i64> {
    // Validate all entities exist
    for entity_id in &hyperedge.entity_ids {
        get_entity(conn, *entity_id)?;
    }

    let entity_ids_json = serde_json::to_string(&hyperedge.entity_ids)?;
    let properties_json = serde_json::to_string(&hyperedge.properties)?;

    let tx = conn.unchecked_transaction()?;

    tx.execute(
        r#"
        INSERT INTO kg_hyperedges (hyperedge_type, entity_ids, weight, arity, properties)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            hyperedge.hyperedge_type,
            entity_ids_json,
            hyperedge.weight,
            hyperedge.arity as i64,
            properties_json
        ],
    )?;

    let hyperedge_id = tx.last_insert_rowid();

    // Insert entity-hyperedge associations
    for (position, entity_id) in hyperedge.entity_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO kg_hyperedge_entities (hyperedge_id, entity_id, position) VALUES (?1, ?2, ?3)",
            params![hyperedge_id, entity_id, position as i64],
        )?;
    }

    tx.commit()?;
    Ok(hyperedge_id)
}

/// Get a hyperedge by ID.
pub fn get_hyperedge(conn: &rusqlite::Connection, id: i64) -> Result<Hyperedge> {
    conn.query_row(
        r#"
        SELECT id, hyperedge_type, entity_ids, weight, arity, properties, created_at, updated_at
        FROM kg_hyperedges WHERE id = ?1
        "#,
        params![id],
        |row| {
            let entity_ids_json: String = row.get(2)?;
            let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_json).unwrap_or_default();

            let properties_json: Option<String> = row.get(5)?;
            let properties: HashMap<String, serde_json::Value> = properties_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            let arity = entity_ids.len();
            Ok(Hyperedge {
                id: Some(row.get(0)?),
                hyperedge_type: row.get(1)?,
                entity_ids,
                weight: row.get(3)?,
                arity,
                properties,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .map_err(|_| Error::HyperedgeNotFound(id))
}

/// List hyperedges with optional filtering.
pub fn list_hyperedges(
    conn: &rusqlite::Connection,
    hyperedge_type: Option<&str>,
    min_arity: Option<usize>,
    max_arity: Option<usize>,
    limit: Option<i64>,
) -> Result<Vec<Hyperedge>> {
    let mut query = "SELECT id, hyperedge_type, entity_ids, weight, arity, properties, created_at, updated_at FROM kg_hyperedges WHERE 1=1".to_string();
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(ht) = hyperedge_type {
        query.push_str(&format!(" AND hyperedge_type = ?{param_idx}"));
        params_vec.push(Box::new(ht.to_string()));
        param_idx += 1;
    }

    if let Some(min) = min_arity {
        query.push_str(&format!(" AND arity >= ?{param_idx}"));
        params_vec.push(Box::new(min as i64));
        param_idx += 1;
    }

    if let Some(max) = max_arity {
        query.push_str(&format!(" AND arity <= ?{param_idx}"));
        params_vec.push(Box::new(max as i64));
        param_idx += 1;
    }

    query.push_str(" ORDER BY created_at DESC");

    if let Some(lim) = limit {
        query.push_str(&format!(" LIMIT ?{param_idx}"));
        params_vec.push(Box::new(lim));
    }

    let mut stmt = conn.prepare(&query)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        let entity_ids_json: String = row.get(2)?;
        let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_json).unwrap_or_default();

        let properties_json: Option<String> = row.get(5)?;
        let properties: HashMap<String, serde_json::Value> = properties_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let arity = entity_ids.len();
        Ok(Hyperedge {
            id: Some(row.get(0)?),
            hyperedge_type: row.get(1)?,
            entity_ids,
            weight: row.get(3)?,
            arity,
            properties,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Update a hyperedge.
pub fn update_hyperedge(conn: &rusqlite::Connection, hyperedge: &Hyperedge) -> Result<()> {
    let id = hyperedge.id.ok_or(Error::HyperedgeNotFound(0))?;

    // Validate all entities exist
    for entity_id in &hyperedge.entity_ids {
        get_entity(conn, *entity_id)?;
    }

    let entity_ids_json = serde_json::to_string(&hyperedge.entity_ids)?;
    let properties_json = serde_json::to_string(&hyperedge.properties)?;

    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let tx = conn.unchecked_transaction()?;

    let affected = tx.execute(
        r#"
        UPDATE kg_hyperedges
        SET hyperedge_type = ?1, entity_ids = ?2, weight = ?3, arity = ?4, properties = ?5, updated_at = ?6
        WHERE id = ?7
        "#,
        params![
            hyperedge.hyperedge_type,
            entity_ids_json,
            hyperedge.weight,
            hyperedge.arity as i64,
            properties_json,
            updated_at,
            id
        ],
    )?;

    if affected == 0 {
        return Err(Error::HyperedgeNotFound(id));
    }

    // Rebuild entity associations
    tx.execute(
        "DELETE FROM kg_hyperedge_entities WHERE hyperedge_id = ?1",
        params![id],
    )?;

    for (position, entity_id) in hyperedge.entity_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO kg_hyperedge_entities (hyperedge_id, entity_id, position) VALUES (?1, ?2, ?3)",
            params![id, entity_id, position as i64],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Delete a hyperedge by ID.
pub fn delete_hyperedge(conn: &rusqlite::Connection, id: i64) -> Result<()> {
    let affected = conn.execute("DELETE FROM kg_hyperedges WHERE id = ?1", params![id])?;
    if affected == 0 {
        return Err(Error::HyperedgeNotFound(id));
    }
    Ok(())
}

/// Get higher-order neighbors of an entity (entities connected through hyperedges).
pub fn get_higher_order_neighbors(
    conn: &rusqlite::Connection,
    entity_id: i64,
    min_arity: Option<usize>,
    max_arity: Option<usize>,
) -> Result<Vec<HigherOrderNeighbor>> {
    // Validate entity exists
    get_entity(conn, entity_id)?;

    let min_arity = min_arity.unwrap_or(2) as i64;
    let max_arity = max_arity.unwrap_or(100) as i64;

    let mut stmt = conn.prepare(
        r#"
        SELECT h.id, h.hyperedge_type, h.entity_ids, h.weight, h.arity, h.properties,
               h.created_at, h.updated_at,
               he2.entity_id as neighbor_id, he2.position
        FROM kg_hyperedge_entities he
        JOIN kg_hyperedges h ON he.hyperedge_id = h.id
        JOIN kg_hyperedge_entities he2 ON h.id = he2.hyperedge_id
        WHERE he.entity_id = ?1
          AND he2.entity_id != ?1
          AND h.arity >= ?2
          AND h.arity <= ?3
        ORDER BY h.weight DESC
        "#,
    )?;

    let rows = stmt.query_map(params![entity_id, min_arity, max_arity], |row| {
        let entity_ids_json: String = row.get(2)?;
        let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_json).unwrap_or_default();

        let properties_json: Option<String> = row.get(5)?;
        let properties: HashMap<String, serde_json::Value> = properties_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let arity = entity_ids.len();
        let neighbor_id: i64 = row.get(8)?;
        let position: i64 = row.get(9)?;

        Ok((
            Hyperedge {
                id: Some(row.get(0)?),
                hyperedge_type: row.get(1)?,
                entity_ids,
                weight: row.get(3)?,
                arity,
                properties,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            },
            neighbor_id,
            position as usize,
        ))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (hyperedge, neighbor_id, position) = row?;
        let entity = get_entity(conn, neighbor_id)?;
        result.push(HigherOrderNeighbor {
            entity,
            hyperedge,
            position: Some(position),
        });
    }

    Ok(result)
}

/// Get all hyperedges that an entity participates in.
pub fn get_entity_hyperedges(
    conn: &rusqlite::Connection,
    entity_id: i64,
) -> Result<Vec<Hyperedge>> {
    get_entity(conn, entity_id)?;

    let mut stmt = conn.prepare(
        r#"
        SELECT h.id, h.hyperedge_type, h.entity_ids, h.weight, h.arity, h.properties,
               h.created_at, h.updated_at
        FROM kg_hyperedge_entities he
        JOIN kg_hyperedges h ON he.hyperedge_id = h.id
        WHERE he.entity_id = ?1
        ORDER BY h.created_at DESC
        "#,
    )?;

    let rows = stmt.query_map(params![entity_id], |row| {
        let entity_ids_json: String = row.get(2)?;
        let entity_ids: Vec<i64> = serde_json::from_str(&entity_ids_json).unwrap_or_default();

        let properties_json: Option<String> = row.get(5)?;
        let properties: HashMap<String, serde_json::Value> = properties_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let arity = entity_ids.len();
        Ok(Hyperedge {
            id: Some(row.get(0)?),
            hyperedge_type: row.get(1)?,
            entity_ids,
            weight: row.get(3)?,
            arity,
            properties,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Higher-order BFS traversal through hyperedges.
pub fn higher_order_bfs(
    conn: &rusqlite::Connection,
    start_id: i64,
    max_depth: u32,
    min_arity: Option<usize>,
) -> Result<Vec<crate::graph::traversal::TraversalNode>> {
    use crate::graph::traversal::TraversalNode;

    if max_depth == 0 {
        return Ok(Vec::new());
    }
    if max_depth > 10 {
        return Err(Error::InvalidDepth(max_depth));
    }

    get_entity(conn, start_id)?;

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut result = Vec::new();

    visited.insert(start_id);
    queue.push_back((start_id, 0u32));

    while let Some((current_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        let neighbors = get_higher_order_neighbors(conn, current_id, min_arity, None)?;

        for neighbor in neighbors {
            let neighbor_id = neighbor.entity.id.ok_or(Error::EntityNotFound(0))?;
            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id);
                queue.push_back((neighbor_id, depth + 1));
                result.push(TraversalNode {
                    entity_id: neighbor_id,
                    entity_type: neighbor.entity.entity_type.clone(),
                    depth: depth + 1,
                });
            }
        }
    }

    Ok(result)
}

/// Find shortest path between two entities through hyperedges.
pub fn higher_order_shortest_path(
    conn: &rusqlite::Connection,
    from_id: i64,
    to_id: i64,
    max_depth: u32,
) -> Result<Option<HigherOrderPath>> {
    if max_depth == 0 {
        return Ok(None);
    }
    if max_depth > 10 {
        return Err(Error::InvalidDepth(max_depth));
    }

    get_entity(conn, from_id)?;
    get_entity(conn, to_id)?;

    if from_id == to_id {
        return Ok(Some(HigherOrderPath {
            steps: Vec::new(),
            total_weight: 0.0,
        }));
    }

    let mut visited = HashSet::new();
    let mut queue: VecDeque<(i64, u32)> = VecDeque::new();
    // parent map: entity_id -> (parent_entity_id, hyperedge used)
    let mut parent: HashMap<i64, (i64, Hyperedge)> = HashMap::new();

    visited.insert(from_id);
    queue.push_back((from_id, 0));

    while let Some((current_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        let neighbors = get_higher_order_neighbors(conn, current_id, None, None)?;

        for neighbor in neighbors {
            let neighbor_id = neighbor.entity.id.ok_or(Error::EntityNotFound(0))?;
            if !visited.contains(&neighbor_id) {
                visited.insert(neighbor_id);
                parent.insert(neighbor_id, (current_id, neighbor.hyperedge));
                if neighbor_id == to_id {
                    // Reconstruct path
                    return Ok(Some(reconstruct_path(&parent, from_id, to_id)));
                }
                queue.push_back((neighbor_id, depth + 1));
            }
        }
    }

    Ok(None)
}

fn reconstruct_path(
    parent: &HashMap<i64, (i64, Hyperedge)>,
    from_id: i64,
    to_id: i64,
) -> HigherOrderPath {
    let mut steps = Vec::new();
    let mut current = to_id;
    let mut total_weight = 0.0;

    while current != from_id {
        // parent was populated for every node we visited, so this entry
        // is guaranteed to exist; using if-let avoids an unwrap panic.
        if let Some((prev, hyperedge)) = parent.get(&current) {
            total_weight += hyperedge.weight;
            steps.push(HigherOrderPathStep {
                hyperedge: hyperedge.clone(),
                from_entity: *prev,
                to_entity: current,
            });
            current = *prev;
        } else {
            break; // defensive: should never happen in a well-formed graph
        }
    }

    steps.reverse();
    HigherOrderPath {
        steps,
        total_weight,
    }
}

/// Compute hyperedge degree centrality for an entity.
pub fn hyperedge_degree(conn: &rusqlite::Connection, entity_id: i64) -> Result<f64> {
    get_entity(conn, entity_id)?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT hyperedge_id) FROM kg_hyperedge_entities WHERE entity_id = ?1",
        params![entity_id],
        |row| row.get(0),
    )?;

    Ok(count as f64)
}

/// Load all hyperedges from the database.
pub fn load_all_hyperedges(conn: &rusqlite::Connection) -> Result<Vec<Hyperedge>> {
    list_hyperedges(conn, None, None, None, None)
}

/// Compute entity-level hypergraph PageRank using Zhou formula.
///
/// Based on Zhou et al. (2006) - "Learning with Hypergraphs".
///
/// PR(v) = (1-d)/n + d * sum_{e: v in e} [w(e)/delta(e) * sum_{u in e, u!=v} PR(u) * (1/d(u)) * (1/delta(e))]
///
/// Simplified: PR(v) = (1-d)/n + d * sum_{e: v in e} [w(e)/delta(e)^2 * sum_{u in e, u!=v} PR(u)/d(u)]
///
/// Complexity: O(T * sum_e k_e^2), much faster than naive O(n^2) approaches.
pub fn hypergraph_entity_pagerank(
    conn: &rusqlite::Connection,
    damping: f64,
    max_iter: usize,
    tolerance: f64,
) -> Result<HashMap<i64, f64>> {
    let hyperedges = load_all_hyperedges(conn)?;

    if hyperedges.is_empty() {
        return Ok(HashMap::new());
    }

    // Collect all entity IDs that appear in hyperedges
    let mut all_entities: HashSet<i64> = HashSet::new();
    for he in &hyperedges {
        for &eid in &he.entity_ids {
            all_entities.insert(eid);
        }
    }

    let n = all_entities.len() as f64;
    if n == 0.0 {
        return Ok(HashMap::new());
    }

    // Compute hyperedge degree d(v) for each entity
    // d(v) = number of hyperedges containing v
    let mut entity_degree: HashMap<i64, usize> = HashMap::new();
    for he in &hyperedges {
        for &eid in &he.entity_ids {
            *entity_degree.entry(eid).or_insert(0) += 1;
        }
    }

    // Initialize PageRank scores uniformly
    let mut scores: HashMap<i64, f64> = all_entities.iter().map(|&id| (id, 1.0 / n)).collect();

    // Iterative update using Zhou formula
    for _ in 0..max_iter {
        let mut new_scores: HashMap<i64, f64> = HashMap::new();

        // Initialize with random jump term
        for &eid in &all_entities {
            new_scores.insert(eid, (1.0 - damping) / n);
        }

        // For each hyperedge e, compute contribution to its entities
        for he in &hyperedges {
            let w_e = he.weight;
            let delta_e = he.arity as f64;
            // Zhou formula uses 1/delta(e) for each vertex in the hyperedge
            let inv_delta = 1.0 / delta_e;

            // Compute sum of PR(u)/d(u) for all u in e
            let sum_pr_d: f64 = he
                .entity_ids
                .iter()
                .map(|&u| {
                    let d_u = *entity_degree.get(&u).unwrap_or(&1) as f64;
                    let pr_u = scores.get(&u).copied().unwrap_or(0.0);
                    pr_u / d_u
                })
                .sum();

            // For each v in e, add contribution from other vertices
            for &v in &he.entity_ids {
                let d_v = *entity_degree.get(&v).unwrap_or(&1) as f64;
                let pr_v = scores.get(&v).copied().unwrap_or(0.0);

                // Subtract v's own contribution to get sum of u != v
                let sum_pr_d_excluding_v = sum_pr_d - pr_v / d_v;

                // Zhou formula: w(e) / delta(e)^2 * sum_{u != v} PR(u)/d(u)
                let contribution = damping * w_e * inv_delta * inv_delta * sum_pr_d_excluding_v;

                *new_scores.entry(v).or_insert(0.0) += contribution;
            }
        }

        // Normalize scores to ensure sum = 1.0
        let total: f64 = new_scores.values().sum();
        if total > 0.0 {
            for score in new_scores.values_mut() {
                *score /= total;
            }
        }

        // Check convergence
        let diff: f64 = all_entities
            .iter()
            .map(|id| (new_scores.get(id).unwrap_or(&0.0) - scores.get(id).unwrap_or(&0.0)).abs())
            .sum();

        scores = new_scores;

        if diff < tolerance {
            break;
        }
    }

    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::entity::insert_entity;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        crate::schema::create_schema(&conn).unwrap();
        conn
    }

    fn create_test_entities(conn: &Connection, count: usize) -> Vec<i64> {
        (0..count)
            .map(|i| insert_entity(conn, &Entity::new("person", format!("Person {i}"))).unwrap())
            .collect()
    }

    #[test]
    fn test_hyperedge_creation() {
        let he = Hyperedge::new(vec![1, 2, 3], "collaboration", 0.8).unwrap();
        assert_eq!(he.arity, 3);
        assert!(he.contains(1));
        assert!(he.contains(2));
        assert!(he.contains(3));
        assert!(!he.contains(4));
    }

    #[test]
    fn test_hyperedge_invalid_arity() {
        let result = Hyperedge::new(vec![1], "test", 0.5);
        assert!(result.is_err());

        let result = Hyperedge::new(vec![], "test", 0.5);
        assert!(result.is_err());
    }

    #[test]
    fn test_hyperedge_invalid_weight() {
        let result = Hyperedge::new(vec![1, 2], "test", 1.5);
        assert!(result.is_err());

        let result = Hyperedge::new(vec![1, 2], "test", -0.1);
        assert!(result.is_err());
    }

    #[test]
    fn test_hyperedge_intersection() {
        let he1 = Hyperedge::new(vec![1, 2, 3], "a", 0.5).unwrap();
        let he2 = Hyperedge::new(vec![2, 3, 4], "b", 0.5).unwrap();
        let mut inter = he1.intersection(&he2);
        inter.sort();
        assert_eq!(inter, vec![2, 3]);
        assert!(he1.has_intersection(&he2));
    }

    #[test]
    fn test_hyperedge_no_intersection() {
        let he1 = Hyperedge::new(vec![1, 2], "a", 0.5).unwrap();
        let he2 = Hyperedge::new(vec![3, 4], "b", 0.5).unwrap();
        assert!(he1.intersection(&he2).is_empty());
        assert!(!he1.has_intersection(&he2));
    }

    #[test]
    fn test_insert_and_get_hyperedge() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 3);

        let he = Hyperedge::new(ids.clone(), "collaboration", 0.8).unwrap();
        let he_id = insert_hyperedge(&conn, &he).unwrap();
        assert!(he_id > 0);

        let retrieved = get_hyperedge(&conn, he_id).unwrap();
        assert_eq!(retrieved.arity, 3);
        assert_eq!(retrieved.hyperedge_type, "collaboration");
        assert_eq!(retrieved.entity_ids, ids);
        assert!((retrieved.weight - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_list_hyperedges() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 5);

        insert_hyperedge(
            &conn,
            &Hyperedge::new(ids[0..3].to_vec(), "team", 0.9).unwrap(),
        )
        .unwrap();
        insert_hyperedge(
            &conn,
            &Hyperedge::new(ids[2..5].to_vec(), "team", 0.8).unwrap(),
        )
        .unwrap();
        insert_hyperedge(&conn, &Hyperedge::new(ids.clone(), "project", 0.7).unwrap()).unwrap();

        let all = list_hyperedges(&conn, None, None, None, None).unwrap();
        assert_eq!(all.len(), 3);

        let teams = list_hyperedges(&conn, Some("team"), None, None, None).unwrap();
        assert_eq!(teams.len(), 2);

        let big = list_hyperedges(&conn, None, Some(4), None, None).unwrap();
        assert_eq!(big.len(), 1);
    }

    #[test]
    fn test_update_hyperedge() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 4);

        let he = Hyperedge::new(ids[0..3].to_vec(), "team", 0.9).unwrap();
        let he_id = insert_hyperedge(&conn, &he).unwrap();

        let mut updated = get_hyperedge(&conn, he_id).unwrap();
        updated.entity_ids = ids.clone();
        updated.arity = ids.len();
        updated.weight = 0.7;
        update_hyperedge(&conn, &updated).unwrap();

        let retrieved = get_hyperedge(&conn, he_id).unwrap();
        assert_eq!(retrieved.arity, 4);
        assert!((retrieved.weight - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_delete_hyperedge() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 3);

        let he = Hyperedge::new(ids, "team", 0.9).unwrap();
        let he_id = insert_hyperedge(&conn, &he).unwrap();

        delete_hyperedge(&conn, he_id).unwrap();
        assert!(get_hyperedge(&conn, he_id).is_err());
    }

    #[test]
    fn test_delete_hyperedge_not_found() {
        let conn = setup_db();
        assert!(delete_hyperedge(&conn, 999).is_err());
    }

    #[test]
    fn test_higher_order_neighbors() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 5);

        // Team 1: Person 0, 1, 2
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[1], ids[2]], "team", 0.9).unwrap(),
        )
        .unwrap();

        // Team 2: Person 2, 3, 4
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[2], ids[3], ids[4]], "team", 0.8).unwrap(),
        )
        .unwrap();

        // Neighbors of Person 0 through hyperedges
        let neighbors = get_higher_order_neighbors(&conn, ids[0], None, None).unwrap();
        assert_eq!(neighbors.len(), 2); // Person 1, Person 2

        let neighbor_ids: HashSet<i64> = neighbors.iter().map(|n| n.entity.id.unwrap()).collect();
        assert!(neighbor_ids.contains(&ids[1]));
        assert!(neighbor_ids.contains(&ids[2]));
    }

    #[test]
    fn test_entity_hyperedges() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 4);

        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[1], ids[2]], "team", 0.9).unwrap(),
        )
        .unwrap();
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[3]], "pair", 0.5).unwrap(),
        )
        .unwrap();

        let hyperedges = get_entity_hyperedges(&conn, ids[0]).unwrap();
        assert_eq!(hyperedges.len(), 2);
    }

    #[test]
    fn test_higher_order_bfs() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 5);

        // Chain: {0,1,2} -- {2,3,4}
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[1], ids[2]], "team", 0.9).unwrap(),
        )
        .unwrap();
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[2], ids[3], ids[4]], "team", 0.8).unwrap(),
        )
        .unwrap();

        let traversal = higher_order_bfs(&conn, ids[0], 2, None).unwrap();
        let traversed_ids: HashSet<i64> = traversal.iter().map(|n| n.entity_id).collect();

        // Should reach all other entities through the chain
        assert!(traversed_ids.contains(&ids[1]));
        assert!(traversed_ids.contains(&ids[2]));
        assert!(traversed_ids.contains(&ids[3]));
        assert!(traversed_ids.contains(&ids[4]));
    }

    #[test]
    fn test_higher_order_shortest_path() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 5);

        // {0,1,2} and {2,3,4}
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[1], ids[2]], "team", 0.9).unwrap(),
        )
        .unwrap();
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[2], ids[3], ids[4]], "team", 0.8).unwrap(),
        )
        .unwrap();

        // Path from 0 to 4 should go through entity 2
        let path = higher_order_shortest_path(&conn, ids[0], ids[4], 5)
            .unwrap()
            .unwrap();
        assert_eq!(path.steps.len(), 2);

        // No path if max_depth is too small
        let path = higher_order_shortest_path(&conn, ids[0], ids[4], 0).unwrap();
        assert!(path.is_none());
    }

    #[test]
    fn test_hyperedge_degree() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 4);

        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[1], ids[2]], "team", 0.9).unwrap(),
        )
        .unwrap();
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[3]], "pair", 0.5).unwrap(),
        )
        .unwrap();

        assert!((hyperedge_degree(&conn, ids[0]).unwrap() - 2.0).abs() < f64::EPSILON);
        assert!((hyperedge_degree(&conn, ids[1]).unwrap() - 1.0).abs() < f64::EPSILON);
        assert!((hyperedge_degree(&conn, ids[3]).unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hypergraph_entity_pagerank() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 5);

        // {0,1,2} and {2,3,4} - entity 2 is the bridge
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[0], ids[1], ids[2]], "team", 0.9).unwrap(),
        )
        .unwrap();
        insert_hyperedge(
            &conn,
            &Hyperedge::new(vec![ids[2], ids[3], ids[4]], "team", 0.8).unwrap(),
        )
        .unwrap();

        let scores = hypergraph_entity_pagerank(&conn, 0.85, 100, 1e-6).unwrap();

        // All 5 entities should have scores
        assert_eq!(scores.len(), 5);

        // Entity 2 (bridge) should have highest score
        let score_2 = scores[&ids[2]];
        for &id in &ids {
            if id != ids[2] {
                assert!(
                    score_2 >= scores[&id],
                    "Bridge entity should have highest PageRank"
                );
            }
        }

        // Scores should sum to approximately 1.0
        let total: f64 = scores.values().sum();
        assert!(
            (total - 1.0).abs() < 0.01,
            "PageRank scores should sum to ~1.0, got {total}"
        );
    }

    #[test]
    fn test_hypergraph_pagerank_empty() {
        let conn = setup_db();
        let scores = hypergraph_entity_pagerank(&conn, 0.85, 100, 1e-6).unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_hyperedge_properties() {
        let conn = setup_db();
        let ids = create_test_entities(&conn, 3);

        let mut he = Hyperedge::new(ids, "team", 0.9).unwrap();
        he.set_property("project", serde_json::json!("Alpha"));
        he.set_property("start_date", serde_json::json!("2026-01-01"));

        let he_id = insert_hyperedge(&conn, &he).unwrap();
        let retrieved = get_hyperedge(&conn, he_id).unwrap();

        assert_eq!(
            retrieved.get_property("project"),
            Some(&serde_json::json!("Alpha"))
        );
        assert_eq!(
            retrieved.get_property("start_date"),
            Some(&serde_json::json!("2026-01-01"))
        );
    }
}
