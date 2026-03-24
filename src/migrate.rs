//! Data migration module for importing external knowledge sources.

use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::graph::Entity;
use crate::KnowledgeGraph;

/// Migrate papers from Aerial's knowledge database to the knowledge graph.
///
/// This function:
/// - Creates "paper" entities from the papers table
/// - Stores arxiv_id and other metadata as properties
/// - Creates placeholder vectors (can be updated later with real embeddings)
pub fn migrate_papers(source_db: &str, kg: &KnowledgeGraph) -> Result<i64> {
    let source_conn = Connection::open(source_db)?;

    let tx = kg.transaction()?;
    let mut count = 0;

    // Query all papers
    let mut stmt = source_conn.prepare(
        r#"
        SELECT arxiv_id, title, file_path, keywords, utility,
               skill_created, last_accessed, created_at, notes
        FROM papers
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,  // arxiv_id
            row.get::<_, String>(1)?,  // title
            row.get::<_, Option<String>>(2)?,  // file_path
            row.get::<_, Option<String>>(3)?,  // keywords
            row.get::<_, Option<f64>>(4)?,  // utility
            row.get::<_, Option<String>>(5)?,  // skill_created
            row.get::<_, Option<String>>(6)?,  // last_accessed
            row.get::<_, Option<String>>(7)?,  // created_at
            row.get::<_, Option<String>>(8)?,  // notes
        ))
    })?;

    for row in rows {
        let (arxiv_id, title, file_path, keywords, utility, skill_created, last_accessed, created_at, notes) =
            row?;

        let mut properties = HashMap::new();
        properties.insert("arxiv_id".to_string(), Value::String(arxiv_id.clone()));

        if let Some(fp) = file_path {
            properties.insert("file_path".to_string(), Value::String(fp));
        }

        if let Some(kw) = keywords {
            properties.insert("keywords".to_string(), Value::String(kw));
        }

        if let Some(util) = utility {
            properties.insert("utility".to_string(), Value::Number(
                serde_json::Number::from_f64(util).unwrap_or(serde_json::Number::from(0))
            ));
        }

        if let Some(skill) = skill_created {
            properties.insert("skill_created".to_string(), Value::String(skill));
        }

        if let Some(la) = last_accessed {
            properties.insert("last_accessed".to_string(), Value::String(la));
        }

        if let Some(ca) = created_at {
            properties.insert("created_at".to_string(), Value::String(ca));
        }

        if let Some(n) = notes {
            properties.insert("notes".to_string(), Value::String(n));
        }

        let entity = Entity::with_properties("paper", title, properties);

        let entity_id = crate::graph::insert_entity(&tx, &entity)?;
        count += 1;

        // Create placeholder vector (random values, will be replaced later)
        let placeholder_vector = vec![0.0_f32; 384]; // Common embedding dimension
        crate::vector::VectorStore::new().insert_vector(&tx, entity_id, placeholder_vector)?;
    }

    tx.commit()?;
    Ok(count)
}

/// Migrate skills from the skills directory to the knowledge graph.
///
/// This function:
/// - Creates "skill" entities from skill directories
/// - Reads SKILL.md files for content
/// - Creates placeholder vectors
pub fn migrate_skills(skills_dir: &str, kg: &KnowledgeGraph) -> Result<i64> {
    let skills_path = Path::new(skills_dir);

    if !skills_path.exists() {
        return Err(Error::Other(format!("Skills directory not found: {}", skills_dir)));
    }

    let tx = kg.transaction()?;
    let mut count = 0;

    for entry in fs::read_dir(skills_path)? {
        let entry = entry?;
        let skill_dir = entry.path();

        if skill_dir.is_dir() {
            let skill_name = skill_dir.file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Error::Other("Invalid skill directory name".to_string()))?;

            let mut properties = HashMap::new();
            properties.insert("skill_name".to_string(), Value::String(skill_name.to_string()));

            // Try to read SKILL.md
            let skill_md_path = skill_dir.join("SKILL.md");
            if skill_md_path.exists() {
                let content = fs::read_to_string(&skill_md_path).unwrap_or_default();

                // Extract metadata from SKILL.md
                if let Some(description) = extract_description(&content) {
                    properties.insert("description".to_string(), Value::String(description));
                }

                properties.insert("content".to_string(), Value::String(content));
            }

            let entity = Entity::with_properties("skill", skill_name, properties);
            let entity_id = crate::graph::insert_entity(&tx, &entity)?;
            count += 1;

            // Create placeholder vector
            let placeholder_vector = vec![0.0_f32; 384];
            crate::vector::VectorStore::new().insert_vector(&tx, entity_id, placeholder_vector)?;
        }
    }

    tx.commit()?;
    Ok(count)
}

/// Build relationships between entities.
///
/// This function:
/// - Links papers to skills (derived_from)
/// - Links related papers (related_by_keywords)
/// - Links similar skills (similar_to)
pub fn build_relationships(kg: &KnowledgeGraph) -> Result<i64> {
    let tx = kg.transaction()?;
    let mut count = 0;

    // Get all papers and skills
    let papers = kg.list_entities(Some("paper"), None)?;
    let skills = kg.list_entities(Some("skill"), None)?;

    // Build arxiv_id -> entity_id map for papers
    let mut paper_map: HashMap<String, i64> = HashMap::new();
    for paper in &papers {
        if let Some(arxiv_id) = paper.get_property("arxiv_id").and_then(|v| v.as_str()) {
            if let Some(id) = paper.id {
                paper_map.insert(arxiv_id.to_string(), id);
            }
        }
    }

    // Build skill_name -> entity_id map for skills
    let mut skill_map: HashMap<String, i64> = HashMap::new();
    for skill in &skills {
        if let Some(skill_name) = skill.get_property("skill_name").and_then(|v| v.as_str()) {
            if let Some(id) = skill.id {
                skill_map.insert(skill_name.to_string(), id);
            }
        }
    }

    // Connect papers to skills (derived_from)
    for paper in &papers {
        if let Some(skill_created) = paper.get_property("skill_created").and_then(|v| v.as_str()) {
            if !skill_created.is_empty() {
                if let Some(paper_id) = paper.id {
                    if let Some(skill_id) = skill_map.get(skill_created) {
                        let relation = crate::graph::Relation::new(paper_id, *skill_id, "derived_from", 1.0)?;
                        crate::graph::insert_relation(&tx, &relation)?;
                        count += 1;
                    }
                }
            }
        }
    }

    // Connect related papers (related_by_keywords)
    for i in 0..papers.len() {
        for j in (i + 1)..papers.len() {
            let paper_a = &papers[i];
            let paper_b = &papers[j];

            if let (Some(id_a), Some(id_b)) = (paper_a.id, paper_b.id) {
                if let Some(similarity) = compute_keyword_similarity(paper_a, paper_b) {
                    if similarity > 0.3 {
                        let relation = crate::graph::Relation::new(id_a, id_b, "related_by_keywords", similarity)?;
                        crate::graph::insert_relation(&tx, &relation)?;
                        count += 1;
                    }
                }
            }
        }
    }

    // Connect similar skills (similar_to) - based on common keywords in descriptions
    for i in 0..skills.len() {
        for j in (i + 1)..skills.len() {
            let skill_a = &skills[i];
            let skill_b = &skills[j];

            if let (Some(id_a), Some(id_b)) = (skill_a.id, skill_b.id) {
                if let Some(similarity) = compute_skill_similarity(skill_a, skill_b) {
                    if similarity > 0.3 {
                        let relation = crate::graph::Relation::new(id_a, id_b, "similar_to", similarity)?;
                        crate::graph::insert_relation(&tx, &relation)?;
                        count += 1;
                    }
                }
            }
        }
    }

    tx.commit()?;
    Ok(count)
}

/// Extract description from SKILL.md content.
fn extract_description(content: &str) -> Option<String> {
    // Look for description section or use first paragraph
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("# Description") || line.starts_with("## Description") {
            continue;
        }
        if !line.is_empty() && !line.starts_with("#") {
            return Some(line.to_string());
        }
    }
    None
}

/// Compute similarity between two papers based on keywords.
fn compute_keyword_similarity(paper_a: &Entity, paper_b: &Entity) -> Option<f64> {
    let keywords_a: Vec<String> = paper_a
        .get_property("keywords")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();

    let keywords_b: Vec<String> = paper_b
        .get_property("keywords")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();

    if keywords_a.is_empty() || keywords_b.is_empty() {
        return None;
    }

    let set_a: std::collections::HashSet<&String> = keywords_a.iter().collect();
    let set_b: std::collections::HashSet<&String> = keywords_b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        return Some(0.0);
    }

    Some(intersection as f64 / union as f64)
}

/// Compute similarity between two skills based on description content.
fn compute_skill_similarity(skill_a: &Entity, skill_b: &Entity) -> Option<f64> {
    let desc_a = skill_a.get_property("description").and_then(|v| v.as_str()).unwrap_or("");
    let desc_b = skill_b.get_property("description").and_then(|v| v.as_str()).unwrap_or("");

    if desc_a.is_empty() || desc_b.is_empty() {
        return None;
    }

    // Simple word overlap similarity
    let words_a: std::collections::HashSet<&str> =
        desc_a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> =
        desc_b.split_whitespace().collect();

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
        return Some(0.0);
    }

    Some(intersection as f64 / union as f64)
}

/// Perform full migration: papers, skills, and relationships.
pub fn migrate_all(source_db: &str, skills_dir: &str, kg: &KnowledgeGraph) -> Result<MigrationStats> {
    let papers_count = migrate_papers(source_db, kg)?;
    let skills_count = migrate_skills(skills_dir, kg)?;
    let relations_count = build_relationships(kg)?;

    Ok(MigrationStats {
        papers_count,
        skills_count,
        relations_count,
    })
}

/// Statistics from migration.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationStats {
    pub papers_count: i64,
    pub skills_count: i64,
    pub relations_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_description() {
        let content = "# Test Skill\n\nThis is a test description.\n\nMore content here.";
        let desc = extract_description(content);
        assert_eq!(desc, Some("This is a test description.".to_string()));
    }

    #[test]
    fn test_keyword_similarity() {
        let mut paper_a = Entity::new("paper", "Paper A");
        // Store as JSON string (as it comes from the database)
        paper_a.set_property("keywords", serde_json::Value::String(r#"["machine", "learning"]"#.to_string()));

        let mut paper_b = Entity::new("paper", "Paper B");
        paper_b.set_property("keywords", serde_json::Value::String(r#"["machine", "vision"]"#.to_string()));

        let similarity = compute_keyword_similarity(&paper_a, &paper_b).unwrap();
        // keywords_a = ["machine", "learning"]
        // keywords_b = ["machine", "vision"]
        // intersection = ["machine"] = 1
        // union = ["machine", "learning", "vision"] = 3
        // similarity = 1/3 ≈ 0.333
        assert!((similarity - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_skill_similarity() {
        let mut skill_a = Entity::new("skill", "Skill A");
        skill_a.set_property("description", serde_json::json!("neural network learning"));

        let mut skill_b = Entity::new("skill", "Skill B");
        skill_b.set_property("description", serde_json::json!("neural network vision"));

        let similarity = compute_skill_similarity(&skill_a, &skill_b).unwrap();
        // Intersection: {neural, network} = 2 words
        // Union: {neural, network, learning, vision} = 4 words
        // Similarity: 2/4 = 0.5
        assert!((similarity - 0.5).abs() < 0.01);
    }
}
