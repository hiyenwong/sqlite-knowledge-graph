//! Vector embedding generation module for semantic search.

use crate::error::{Error, Result};
use crate::vector::VectorStore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

/// Embedding model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub model_name: String,
    pub dimension: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "all-MiniLM-L6-v2".to_string(),
            dimension: 384,
        }
    }
}

/// Embedding generator using sentence-transformers.
pub struct EmbeddingGenerator {
    config: EmbeddingConfig,
    /// If true, skip entities that already have real (non-zero) embeddings.
    pub skip_existing: bool,
}

impl EmbeddingGenerator {
    /// Create a new embedding generator with default configuration.
    pub fn new() -> Self {
        Self {
            config: EmbeddingConfig::default(),
            skip_existing: true,
        }
    }

    /// Create a new embedding generator with custom configuration.
    pub fn with_config(config: EmbeddingConfig) -> Self {
        Self {
            config,
            skip_existing: true,
        }
    }

    /// Set force mode: if true, regenerate embeddings even for entities that
    /// already have real (non-zero) vectors.
    pub fn with_force(mut self, force: bool) -> Self {
        self.skip_existing = !force;
        self
    }

    /// Generate embeddings for a list of texts.
    pub fn generate_embeddings(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let python_script = self.generate_python_script()?;

        // Serialize texts to JSON
        let texts_json = serde_json::to_string(&texts)
            .map_err(|e| Error::Other(format!("Failed to serialize texts: {}", e)))?;

        // Run Python script with stdin
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(&python_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Other(format!("Failed to spawn Python: {}", e)))?;

        // Write to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(texts_json.as_bytes())
                .map_err(|e| Error::Other(format!("Failed to write to stdin: {}", e)))?;
        }

        // Get the output
        let output = child
            .wait_with_output()
            .map_err(|e| Error::Other(format!("Failed to read Python output: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Other(format!("Python script failed: {}", stderr)));
        }

        // Parse output
        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_embeddings(&stdout)
    }

    /// Generate Python script for embedding generation.
    fn generate_python_script(&self) -> Result<String> {
        let script = format!(
            r#"
import sys
import json
import numpy as np

try:
    from sentence_transformers import SentenceTransformer

    # Load model
    model = SentenceTransformer('{}')

    # Read texts from stdin
    texts_json = sys.stdin.read()
    texts = json.loads(texts_json)

    # Generate embeddings
    embeddings = model.encode(texts, convert_to_numpy=True)

    # Convert to list and print as JSON
    embeddings_list = embeddings.tolist()
    print(json.dumps(embeddings_list))

except ImportError:
    print("{{\"error\": \"sentence-transformers not installed. Run: pip install sentence-transformers\"}}", file=sys.stderr)
    sys.exit(1)
except Exception as e:
    print("{{\"error\": \"{{}}\"}}".format(str(e)), file=sys.stderr)
    sys.exit(1)
"#,
            self.config.model_name
        );

        Ok(script)
    }

    /// Parse embeddings from Python output.
    fn parse_embeddings(&self, output: &str) -> Result<Vec<Vec<f32>>> {
        let embeddings: Vec<Vec<f32>> = serde_json::from_str(output)
            .map_err(|e| Error::Other(format!("Failed to parse embeddings: {}", e)))?;

        // Validate dimensions
        for embedding in &embeddings {
            if embedding.len() != self.config.dimension {
                return Err(Error::InvalidVectorDimension {
                    expected: self.config.dimension,
                    actual: embedding.len(),
                });
            }
        }

        Ok(embeddings)
    }

    /// Generate embeddings for all paper entities in the database.
    pub fn generate_for_papers(&self, conn: &Connection) -> Result<EmbeddingStats> {
        let entities = get_entities_needing_embedding(conn, "paper", !self.skip_existing)?;
        let total_count = count_entities(conn, "paper")?;
        let skipped_count = total_count - entities.len() as i64;

        self.generate_and_store(conn, entities, total_count, skipped_count, "paper")
    }

    /// Generate embeddings for all skill entities in the database.
    pub fn generate_for_skills(&self, conn: &Connection) -> Result<EmbeddingStats> {
        let entities = get_entities_needing_embedding(conn, "skill", !self.skip_existing)?;
        let total_count = count_entities(conn, "skill")?;
        let skipped_count = total_count - entities.len() as i64;

        self.generate_and_store(conn, entities, total_count, skipped_count, "skill")
    }

    /// Generate embeddings for all entities in the database.
    pub fn generate_for_all(&self, conn: &Connection) -> Result<EmbeddingStats> {
        let papers_stats = self.generate_for_papers(conn)?;
        let skills_stats = self.generate_for_skills(conn)?;

        Ok(EmbeddingStats {
            total_count: papers_stats.total_count + skills_stats.total_count,
            processed_count: papers_stats.processed_count + skills_stats.processed_count,
            skipped_count: papers_stats.skipped_count + skills_stats.skipped_count,
            dimension: self.config.dimension,
        })
    }

    /// Internal: batch-generate embeddings for a list of (id, text) pairs and store them.
    fn generate_and_store(
        &self,
        conn: &Connection,
        entities: Vec<(i64, String)>,
        total_count: i64,
        skipped_count: i64,
        label: &str,
    ) -> Result<EmbeddingStats> {
        if entities.is_empty() {
            println!(
                "All {} entities already have real embeddings, skipping.",
                label
            );
            return Ok(EmbeddingStats {
                total_count,
                processed_count: 0,
                skipped_count,
                dimension: self.config.dimension,
            });
        }

        let (entity_ids, texts): (Vec<i64>, Vec<String>) = entities.into_iter().unzip();

        println!(
            "Generating embeddings for {} {} titles ({} already have real embeddings, skipping)...",
            texts.len(),
            label,
            skipped_count
        );

        let batch_size = 100;
        let mut processed_count = 0;

        let store = VectorStore::new();
        let tx = conn.unchecked_transaction()?;

        for batch_start in (0..texts.len()).step_by(batch_size) {
            let batch_end = (batch_start + batch_size).min(texts.len());
            let batch_texts = texts[batch_start..batch_end].to_vec();
            let batch_ids = entity_ids[batch_start..batch_end].to_vec();

            println!(
                "Processing batch: {}s {}-{}",
                label,
                batch_start + 1,
                batch_end
            );

            let embeddings = self.generate_embeddings(batch_texts)?;

            for (entity_id, embedding) in batch_ids.iter().zip(embeddings.iter()) {
                store.insert_vector(&tx, *entity_id, embedding.clone())?;
            }

            processed_count += embeddings.len();
            println!("  Generated {} embeddings", embeddings.len());
        }

        tx.commit()?;

        println!("✓ Generated {} embeddings for {}s", processed_count, label);

        Ok(EmbeddingStats {
            total_count,
            processed_count: processed_count as i64,
            skipped_count,
            dimension: self.config.dimension,
        })
    }
}

impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Get entity (id, name) pairs that need embeddings generated.
///
/// - If `force` is true, returns all entities of the given type.
/// - Otherwise, returns only entities with missing or placeholder (all-zero) vectors.
pub fn get_entities_needing_embedding(
    conn: &Connection,
    entity_type: &str,
    force: bool,
) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT e.id, e.name, v.vector
        FROM kg_entities e
        LEFT JOIN kg_vectors v ON e.id = v.entity_id
        WHERE e.entity_type = ?1
        ORDER BY e.id
        "#,
    )?;

    let rows = stmt.query_map([entity_type], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<Vec<u8>>>(2)?,
        ))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (id, name, blob) = row?;
        let needs_embedding = force || is_placeholder_or_missing(blob.as_deref());
        if needs_embedding {
            result.push((id, name));
        }
    }

    Ok(result)
}

/// Returns true if the vector blob is missing (None) or all-zero bytes (placeholder).
fn is_placeholder_or_missing(blob: Option<&[u8]>) -> bool {
    match blob {
        None => true,
        Some(b) => b.iter().all(|&x| x == 0),
    }
}

/// Count total entities of a given type.
fn count_entities(conn: &Connection, entity_type: &str) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM kg_entities WHERE entity_type = ?1",
        [entity_type],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Statistics from embedding generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStats {
    pub total_count: i64,
    pub processed_count: i64,
    pub skipped_count: i64,
    pub dimension: usize,
}

/// Check if sentence-transformers is available.
pub fn check_dependencies() -> Result<bool> {
    let output = Command::new("python3")
        .arg("-c")
        .arg("import sentence_transformers")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::Other(format!("Failed to check Python dependencies: {}", e)))?;

    Ok(output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::insert_entity;
    use crate::graph::Entity;
    use crate::schema::create_schema;

    fn make_in_memory_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    // ── Config & constructor ──────────────────────────────────────────────

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.model_name, "all-MiniLM-L6-v2");
        assert_eq!(config.dimension, 384);
    }

    #[test]
    fn test_embedding_generator_new() {
        let generator = EmbeddingGenerator::new();
        assert_eq!(generator.config.model_name, "all-MiniLM-L6-v2");
        assert_eq!(generator.config.dimension, 384);
        assert!(generator.skip_existing);
    }

    #[test]
    fn test_with_force_sets_skip_existing() {
        let gen = EmbeddingGenerator::new().with_force(true);
        assert!(!gen.skip_existing);

        let gen2 = EmbeddingGenerator::new().with_force(false);
        assert!(gen2.skip_existing);
    }

    // ── parse_embeddings ─────────────────────────────────────────────────

    #[test]
    fn test_parse_embeddings_dimension_mismatch() {
        let generator = EmbeddingGenerator::new();
        // 3-element vectors don't match expected 384
        let result = generator.parse_embeddings("[[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_embeddings_valid_384() {
        let generator = EmbeddingGenerator::new();
        let vec384: Vec<f32> = (0..384).map(|i| i as f32 / 1000.0).collect();
        let json = serde_json::to_string(&[&vec384]).unwrap();
        let result = generator.parse_embeddings(&json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 384);
        assert!((result[0][0] - 0.0).abs() < 1e-6);
        assert!((result[0][1] - 0.001).abs() < 1e-6);
    }

    #[test]
    fn test_parse_embeddings_batch_of_three() {
        let generator = EmbeddingGenerator::new();
        let vec384: Vec<f32> = vec![0.5f32; 384];
        let batch = vec![vec384.clone(), vec384.clone(), vec384.clone()];
        let json = serde_json::to_string(&batch).unwrap();
        let result = generator.parse_embeddings(&json).unwrap();
        assert_eq!(result.len(), 3);
        for emb in &result {
            assert_eq!(emb.len(), 384);
        }
    }

    #[test]
    fn test_parse_embeddings_invalid_json() {
        let generator = EmbeddingGenerator::new();
        let result = generator.parse_embeddings("not valid json");
        assert!(result.is_err());
    }

    // ── is_placeholder_or_missing ─────────────────────────────────────────

    #[test]
    fn test_is_placeholder_missing() {
        assert!(is_placeholder_or_missing(None));
    }

    #[test]
    fn test_is_placeholder_zero_bytes() {
        let blob = vec![0u8; 384 * 4];
        assert!(is_placeholder_or_missing(Some(&blob)));
    }

    #[test]
    fn test_is_placeholder_real_vector() {
        // Non-zero blob (real embedding)
        let v: Vec<f32> = vec![0.1f32; 384];
        let mut blob = Vec::with_capacity(384 * 4);
        for &val in &v {
            blob.extend_from_slice(&val.to_le_bytes());
        }
        assert!(!is_placeholder_or_missing(Some(&blob)));
    }

    // ── get_entities_needing_embedding ────────────────────────────────────

    #[test]
    fn test_get_entities_needing_embedding_no_vector() {
        let conn = make_in_memory_conn();

        let e1 = Entity::new("paper", "Paper Without Vector");
        let id1 = insert_entity(&conn, &e1).unwrap();
        let _ = id1;

        let result = get_entities_needing_embedding(&conn, "paper", false).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, "Paper Without Vector");
    }

    #[test]
    fn test_get_entities_needing_embedding_placeholder_vector() {
        let conn = make_in_memory_conn();

        let e1 = Entity::new("paper", "Paper With Placeholder");
        let id1 = insert_entity(&conn, &e1).unwrap();

        // Insert placeholder zero vector
        let placeholder = vec![0.0f32; 384];
        VectorStore::new()
            .insert_vector(&conn, id1, placeholder)
            .unwrap();

        let result = get_entities_needing_embedding(&conn, "paper", false).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_get_entities_needing_embedding_skip_real_vector() {
        let conn = make_in_memory_conn();

        let e1 = Entity::new("paper", "Paper With Real Embedding");
        let id1 = insert_entity(&conn, &e1).unwrap();

        // Insert real non-zero embedding
        let real_embedding = vec![0.1f32; 384];
        VectorStore::new()
            .insert_vector(&conn, id1, real_embedding)
            .unwrap();

        let result = get_entities_needing_embedding(&conn, "paper", false).unwrap();
        // Should be empty: the paper already has a real embedding
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_entities_needing_embedding_force_returns_all() {
        let conn = make_in_memory_conn();

        let e1 = Entity::new("paper", "Paper With Real Embedding");
        let id1 = insert_entity(&conn, &e1).unwrap();

        let real_embedding = vec![0.1f32; 384];
        VectorStore::new()
            .insert_vector(&conn, id1, real_embedding)
            .unwrap();

        // force=true should return all entities regardless of existing vectors
        let result = get_entities_needing_embedding(&conn, "paper", true).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_get_entities_needing_embedding_mixed() {
        let conn = make_in_memory_conn();

        let e1 = Entity::new("paper", "Has Real Embedding");
        let id1 = insert_entity(&conn, &e1).unwrap();
        VectorStore::new()
            .insert_vector(&conn, id1, vec![0.1f32; 384])
            .unwrap();

        let e2 = Entity::new("paper", "Has Placeholder");
        let id2 = insert_entity(&conn, &e2).unwrap();
        VectorStore::new()
            .insert_vector(&conn, id2, vec![0.0f32; 384])
            .unwrap();

        let e3 = Entity::new("paper", "No Vector");
        insert_entity(&conn, &e3).unwrap();

        // Without force: only placeholder and missing should be returned
        let result = get_entities_needing_embedding(&conn, "paper", false).unwrap();
        assert_eq!(result.len(), 2);
        let names: Vec<&str> = result.iter().map(|(_, n)| n.as_str()).collect();
        assert!(names.contains(&"Has Placeholder"));
        assert!(names.contains(&"No Vector"));
        assert!(!names.contains(&"Has Real Embedding"));
    }

    // ── generate_for_papers / generate_for_skills empty DB ───────────────

    #[test]
    fn test_generate_for_papers_empty() {
        let conn = make_in_memory_conn();
        let generator = EmbeddingGenerator::new();
        let stats = generator.generate_for_papers(&conn).unwrap();
        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.processed_count, 0);
        assert_eq!(stats.skipped_count, 0);
    }

    #[test]
    fn test_generate_for_skills_empty() {
        let conn = make_in_memory_conn();
        let generator = EmbeddingGenerator::new();
        let stats = generator.generate_for_skills(&conn).unwrap();
        assert_eq!(stats.total_count, 0);
        assert_eq!(stats.processed_count, 0);
        assert_eq!(stats.skipped_count, 0);
    }

    #[test]
    fn test_generate_for_papers_all_real_embeddings_are_skipped() {
        let conn = make_in_memory_conn();

        // Insert papers with real embeddings
        for i in 0..3 {
            let e = Entity::new("paper", format!("Paper {}", i));
            let id = insert_entity(&conn, &e).unwrap();
            VectorStore::new()
                .insert_vector(&conn, id, vec![0.1f32; 384])
                .unwrap();
        }

        let generator = EmbeddingGenerator::new(); // skip_existing = true
        let stats = generator.generate_for_papers(&conn).unwrap();

        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.processed_count, 0);
        assert_eq!(stats.skipped_count, 3);
    }

    // ── batch boundary test ───────────────────────────────────────────────

    #[test]
    fn test_get_entities_batch_boundary() {
        let conn = make_in_memory_conn();

        // Insert 105 papers (crosses the 100-item batch boundary)
        for i in 0..105 {
            let e = Entity::new("paper", format!("Paper {}", i));
            insert_entity(&conn, &e).unwrap();
        }

        let result = get_entities_needing_embedding(&conn, "paper", false).unwrap();
        assert_eq!(result.len(), 105);
    }

    // ── EmbeddingStats ────────────────────────────────────────────────────

    #[test]
    fn test_embedding_stats_fields() {
        let stats = EmbeddingStats {
            total_count: 100,
            processed_count: 80,
            skipped_count: 20,
            dimension: 384,
        };
        assert_eq!(stats.total_count, 100);
        assert_eq!(stats.processed_count, 80);
        assert_eq!(stats.skipped_count, 20);
        assert_eq!(stats.dimension, 384);
    }
}
