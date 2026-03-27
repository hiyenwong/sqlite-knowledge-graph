//! Graph visualization export module.
//!
//! Supports exporting knowledge graphs to various formats for visualization:
//! - D3.js JSON format (nodes + links + metadata)

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;

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
        let weight: f64 = row.get(3)?;
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
