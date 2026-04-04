//! Graph visualization export module.
//!
//! Supports exporting knowledge graphs to various formats for visualization:
//! - D3.js JSON format (nodes + links + metadata)
//! - DOT (Graphviz) format for graph visualization

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;

// Predefined colors for entity types (cycles if more than 8 types)
const TYPE_COLORS: &[&str] = &[
    "blue", "red", "green", "orange", "purple", "brown", "cyan", "magenta",
];

/// Configuration for DOT format export.
#[derive(Debug, Clone)]
pub struct DotConfig {
    /// Graph layout direction: LR, TB, RL, BT (default: "LR")
    pub rankdir: String,
    /// Node shape (default: "ellipse")
    pub node_shape: String,
    /// Color nodes by entity type (default: true)
    pub color_by_type: bool,
    /// Maximum number of nodes to export (default: None = all)
    pub max_nodes: Option<usize>,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            rankdir: "LR".to_string(),
            node_shape: "ellipse".to_string(),
            color_by_type: true,
            max_nodes: None,
        }
    }
}

/// Export the knowledge graph in DOT (Graphviz) format.
///
/// Generates a DOT format string suitable for rendering with Graphviz tools
/// such as `dot`, `neato`, `fdp`, etc.
///
/// # Example output
/// ```text
/// digraph knowledge_graph {
///     rankdir=LR;
///     node [shape=ellipse];
///     1 [label="Deep Learning" color=blue];
///     1 -> 2 [label="related_to" weight=0.8];
/// }
/// ```
pub fn export_dot(conn: &Connection, config: &DotConfig) -> Result<String> {
    let nodes = query_nodes(conn)?;
    let links = query_links(conn)?;

    let nodes = if let Some(max) = config.max_nodes {
        nodes.into_iter().take(max).collect::<Vec<_>>()
    } else {
        nodes
    };

    // Build set of included node ids for filtering links
    let node_ids: std::collections::HashSet<i64> = nodes.iter().map(|n| n.id).collect();

    // Build type -> color mapping
    let mut type_color_map: HashMap<String, &str> = HashMap::new();
    if config.color_by_type {
        let mut color_idx = 0;
        for node in &nodes {
            type_color_map
                .entry(node.node_type.clone())
                .or_insert_with(|| {
                    let color = TYPE_COLORS[color_idx % TYPE_COLORS.len()];
                    color_idx += 1;
                    color
                });
        }
    }

    let mut dot = String::new();
    dot.push_str("digraph knowledge_graph {\n");
    dot.push_str(&format!("    rankdir={};\n", config.rankdir));
    dot.push_str(&format!("    node [shape={}];\n", config.node_shape));
    dot.push('\n');

    // Emit nodes
    for node in &nodes {
        let label = escape_dot_label(&node.name);
        if config.color_by_type {
            let color = type_color_map
                .get(&node.node_type)
                .copied()
                .unwrap_or("black");
            dot.push_str(&format!(
                "    {} [label=\"{}\" color={}];\n",
                node.id, label, color
            ));
        } else {
            dot.push_str(&format!("    {} [label=\"{}\"];\n", node.id, label));
        }
    }

    dot.push('\n');

    // Emit edges (only for included nodes)
    for link in &links {
        if node_ids.contains(&link.source) && node_ids.contains(&link.target) {
            let rel_label = escape_dot_label(&link.link_type);
            dot.push_str(&format!(
                "    {} -> {} [label=\"{}\" weight={:.4}];\n",
                link.source, link.target, rel_label, link.weight
            ));
        }
    }

    dot.push_str("}\n");
    Ok(dot)
}

/// Escape special characters in DOT label strings.
fn escape_dot_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// A node in the D3.js export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D3Node {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub properties: HashMap<String, serde_json::Value>,
}

/// A link (edge) in the D3.js export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D3Link {
    pub source: i64,
    pub target: i64,
    #[serde(rename = "type")]
    pub link_type: String,
    pub weight: f64,
}

/// Metadata for the exported graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D3ExportMetadata {
    pub node_count: usize,
    pub edge_count: usize,
    pub exported_at: String,
}

/// D3.js force-directed graph export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct D3ExportGraph {
    pub nodes: Vec<D3Node>,
    pub links: Vec<D3Link>,
    pub metadata: D3ExportMetadata,
}

/// Export the knowledge graph in D3.js JSON format.
///
/// Queries all entities and relations from the database and formats them
/// as a D3.js force-directed graph with nodes, links, and metadata.
pub fn export_d3_json(conn: &Connection) -> Result<D3ExportGraph> {
    let nodes = query_nodes(conn)?;
    let links = query_links(conn)?;

    let metadata = D3ExportMetadata {
        node_count: nodes.len(),
        edge_count: links.len(),
        exported_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };

    Ok(D3ExportGraph {
        nodes,
        links,
        metadata,
    })
}

fn query_nodes(conn: &Connection) -> Result<Vec<D3Node>> {
    let mut stmt =
        conn.prepare("SELECT id, entity_type, name, properties FROM kg_entities ORDER BY id")?;

    let rows = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let node_type: String = row.get(1)?;
        let name: String = row.get(2)?;
        let properties_json: String = row.get(3)?;
        Ok((id, node_type, name, properties_json))
    })?;

    let mut nodes = Vec::new();
    for row in rows {
        let (id, node_type, name, properties_json) = row?;
        let properties: HashMap<String, serde_json::Value> =
            serde_json::from_str(&properties_json).unwrap_or_default();
        nodes.push(D3Node {
            id,
            name,
            node_type,
            properties,
        });
    }

    Ok(nodes)
}

fn query_links(conn: &Connection) -> Result<Vec<D3Link>> {
    let mut stmt = conn
        .prepare("SELECT source_id, target_id, rel_type, weight FROM kg_relations ORDER BY id")?;

    let rows = stmt.query_map([], |row| {
        let source: i64 = row.get(0)?;
        let target: i64 = row.get(1)?;
        let link_type: String = row.get(2)?;
        let weight = crate::row_get_weight(row, 3)?;
        Ok((source, target, link_type, weight))
    })?;

    let mut links = Vec::new();
    for row in rows {
        let (source, target, link_type, weight) = row?;
        links.push(D3Link {
            source,
            target,
            link_type,
            weight,
        });
    }

    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entity, KnowledgeGraph, Relation};

    // ===== DOT export tests =====

    #[test]
    fn test_export_dot_empty_graph() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        let config = DotConfig::default();
        let dot = export_dot(kg.connection(), &config).unwrap();

        assert!(dot.contains("digraph knowledge_graph {"));
        assert!(dot.contains("rankdir=LR;"));
        assert!(dot.contains("node [shape=ellipse];"));
        assert!(dot.ends_with("}\n"));
    }

    #[test]
    fn test_export_dot_nodes_and_edges() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        let id1 = kg
            .insert_entity(&Entity::new("concept", "Deep Learning"))
            .unwrap();
        let id2 = kg
            .insert_entity(&Entity::new("concept", "Neural Networks"))
            .unwrap();
        kg.insert_relation(&Relation::new(id1, id2, "related_to", 0.8).unwrap())
            .unwrap();

        let config = DotConfig::default();
        let dot = export_dot(kg.connection(), &config).unwrap();

        assert!(dot.contains("Deep Learning"));
        assert!(dot.contains("Neural Networks"));
        assert!(dot.contains("related_to"));
        assert!(dot.contains(&format!("{} -> {}", id1, id2)));
        assert!(dot.contains("weight=0.8000"));
    }

    #[test]
    fn test_export_dot_rankdir_tb() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        kg.insert_entity(&Entity::new("concept", "AI")).unwrap();

        let config = DotConfig {
            rankdir: "TB".to_string(),
            ..Default::default()
        };
        let dot = export_dot(kg.connection(), &config).unwrap();
        assert!(dot.contains("rankdir=TB;"));
    }

    #[test]
    fn test_export_dot_custom_node_shape() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        kg.insert_entity(&Entity::new("concept", "AI")).unwrap();

        let config = DotConfig {
            node_shape: "box".to_string(),
            ..Default::default()
        };
        let dot = export_dot(kg.connection(), &config).unwrap();
        assert!(dot.contains("node [shape=box];"));
    }

    #[test]
    fn test_export_dot_color_by_type() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        kg.insert_entity(&Entity::new("paper", "Paper A")).unwrap();
        kg.insert_entity(&Entity::new("author", "Alice")).unwrap();

        let config = DotConfig {
            color_by_type: true,
            ..Default::default()
        };
        let dot = export_dot(kg.connection(), &config).unwrap();
        // Two different types should each get a color attribute
        assert!(dot.contains("color="));
    }

    #[test]
    fn test_export_dot_no_color() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        kg.insert_entity(&Entity::new("concept", "AI")).unwrap();

        let config = DotConfig {
            color_by_type: false,
            ..Default::default()
        };
        let dot = export_dot(kg.connection(), &config).unwrap();
        // No color attribute when disabled
        assert!(!dot.contains("color="));
    }

    #[test]
    fn test_export_dot_max_nodes() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        for i in 0..5 {
            kg.insert_entity(&Entity::new("concept", &format!("Concept {i}")))
                .unwrap();
        }

        let config = DotConfig {
            max_nodes: Some(3),
            ..Default::default()
        };
        let dot = export_dot(kg.connection(), &config).unwrap();

        // Only the first 3 nodes should appear
        assert!(dot.contains("Concept 0"));
        assert!(dot.contains("Concept 1"));
        assert!(dot.contains("Concept 2"));
        assert!(!dot.contains("Concept 3"));
        assert!(!dot.contains("Concept 4"));
    }

    #[test]
    fn test_export_dot_edges_filtered_by_max_nodes() {
        let kg = KnowledgeGraph::open_in_memory().unwrap();
        let id1 = kg.insert_entity(&Entity::new("concept", "A")).unwrap();
        let id2 = kg.insert_entity(&Entity::new("concept", "B")).unwrap();
        let id3 = kg.insert_entity(&Entity::new("concept", "C")).unwrap();
        kg.insert_relation(&Relation::new(id1, id2, "link", 1.0).unwrap())
            .unwrap();
        kg.insert_relation(&Relation::new(id2, id3, "link", 1.0).unwrap())
            .unwrap();

        // Only include first 2 nodes; edge id2->id3 should be omitted
        let config = DotConfig {
            max_nodes: Some(2),
            color_by_type: false,
            ..Default::default()
        };
        let dot = export_dot(kg.connection(), &config).unwrap();

        assert!(dot.contains(&format!("{} -> {}", id1, id2)));
        assert!(!dot.contains(&format!("{} -> {}", id2, id3)));
    }

    #[test]
    fn test_escape_dot_label() {
        assert_eq!(escape_dot_label("hello"), "hello");
        assert_eq!(escape_dot_label("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_dot_label("line\nnew"), "line\\nnew");
        assert_eq!(escape_dot_label("back\\slash"), "back\\\\slash");
    }

    fn setup() -> KnowledgeGraph {
        KnowledgeGraph::open_in_memory().unwrap()
    }

    #[test]
    fn test_export_empty_graph() {
        let kg = setup();
        let result = export_d3_json(kg.connection()).unwrap();

        assert_eq!(result.nodes.len(), 0);
        assert_eq!(result.links.len(), 0);
        assert_eq!(result.metadata.node_count, 0);
        assert_eq!(result.metadata.edge_count, 0);
        assert!(!result.metadata.exported_at.is_empty());
    }

    #[test]
    fn test_export_nodes_only() {
        let kg = setup();

        let mut paper = Entity::new("paper", "Deep Learning");
        paper.set_property("year", serde_json::json!(2024));
        kg.insert_entity(&paper).unwrap();

        let result = export_d3_json(kg.connection()).unwrap();

        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.links.len(), 0);
        assert_eq!(result.metadata.node_count, 1);
        assert_eq!(result.metadata.edge_count, 0);

        let node = &result.nodes[0];
        assert_eq!(node.name, "Deep Learning");
        assert_eq!(node.node_type, "paper");
        assert_eq!(node.properties["year"], serde_json::json!(2024));
    }

    #[test]
    fn test_export_nodes_and_links() {
        let kg = setup();

        let id1 = kg.insert_entity(&Entity::new("paper", "Paper A")).unwrap();
        let id2 = kg.insert_entity(&Entity::new("paper", "Paper B")).unwrap();
        kg.insert_relation(&Relation::new(id1, id2, "cites", 0.8).unwrap())
            .unwrap();

        let result = export_d3_json(kg.connection()).unwrap();

        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.links.len(), 1);
        assert_eq!(result.metadata.node_count, 2);
        assert_eq!(result.metadata.edge_count, 1);

        let link = &result.links[0];
        assert_eq!(link.source, id1);
        assert_eq!(link.target, id2);
        assert_eq!(link.link_type, "cites");
        assert!((link.weight - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_export_json_serialization() {
        let kg = setup();

        let id1 = kg
            .insert_entity(&Entity::new("concept", "Neural Networks"))
            .unwrap();
        let id2 = kg
            .insert_entity(&Entity::new("concept", "Deep Learning"))
            .unwrap();
        kg.insert_relation(&Relation::new(id1, id2, "related_to", 0.9).unwrap())
            .unwrap();

        let graph = export_d3_json(kg.connection()).unwrap();
        let json_str = serde_json::to_string_pretty(&graph).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(parsed["nodes"].is_array());
        assert!(parsed["links"].is_array());
        assert!(parsed["metadata"].is_object());
        assert_eq!(parsed["metadata"]["node_count"], 2);
        assert_eq!(parsed["metadata"]["edge_count"], 1);
        assert!(parsed["metadata"]["exported_at"].is_string());

        let nodes = parsed["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["name"], "Neural Networks");
        assert_eq!(nodes[0]["type"], "concept");

        let links = parsed["links"].as_array().unwrap();
        assert_eq!(links[0]["type"], "related_to");
        assert_eq!(links[0]["weight"], 0.9);
    }

    #[test]
    fn test_export_multiple_relations() {
        let kg = setup();

        let id1 = kg.insert_entity(&Entity::new("author", "Alice")).unwrap();
        let id2 = kg.insert_entity(&Entity::new("paper", "Paper X")).unwrap();
        let id3 = kg.insert_entity(&Entity::new("topic", "ML")).unwrap();

        kg.insert_relation(&Relation::new(id1, id2, "wrote", 1.0).unwrap())
            .unwrap();
        kg.insert_relation(&Relation::new(id2, id3, "covers", 0.7).unwrap())
            .unwrap();

        let result = export_d3_json(kg.connection()).unwrap();

        assert_eq!(result.nodes.len(), 3);
        assert_eq!(result.links.len(), 2);
        assert_eq!(result.metadata.node_count, 3);
        assert_eq!(result.metadata.edge_count, 2);
    }
}
