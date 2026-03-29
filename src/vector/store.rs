//! Vector storage module for semantic search.

use crate::error::{Error, Result};
use rusqlite::params;

/// Represents a vector storage for embeddings.
pub struct VectorStore;

impl Default for VectorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorStore {
    /// Create a new vector store.
    pub fn new() -> Self {
        Self
    }

    /// Insert a vector for an entity.
    pub fn insert_vector(
        &self,
        conn: &rusqlite::Connection,
        entity_id: i64,
        vector: Vec<f32>,
    ) -> Result<()> {
        // Validate entity exists
        crate::graph::entity::get_entity(conn, entity_id)?;

        // Check if vector dimension matches existing vectors for consistency
        if let Some(existing_dim) = self.check_dimension(conn)? {
            if existing_dim != vector.len() {
                return Err(Error::InvalidVectorDimension {
                    expected: existing_dim,
                    actual: vector.len(),
                });
            }
        }

        // Serialize vector to bytes
        let mut bytes = Vec::with_capacity(vector.len() * 4);
        for &val in &vector {
            bytes.extend_from_slice(&val.to_le_bytes());
        }

        conn.execute(
            r#"
            INSERT OR REPLACE INTO kg_vectors (entity_id, vector, dimension)
            VALUES (?1, ?2, ?3)
            "#,
            params![entity_id, bytes, vector.len() as i64],
        )?;

        Ok(())
    }

    /// Batch insert vectors.
    pub fn insert_vectors_batch(
        &self,
        conn: &rusqlite::Connection,
        vectors: Vec<(i64, Vec<f32>)>,
    ) -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        for (entity_id, vector) in vectors {
            // Serialize vector to bytes (FK constraint enforces entity existence)
            let mut bytes = Vec::with_capacity(vector.len() * 4);
            for &val in &vector {
                bytes.extend_from_slice(&val.to_le_bytes());
            }

            tx.execute(
                r#"
                INSERT OR REPLACE INTO kg_vectors (entity_id, vector, dimension)
                VALUES (?1, ?2, ?3)
                "#,
                params![entity_id, bytes, vector.len() as i64],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Search for similar vectors using cosine similarity.
    pub fn search_vectors(
        &self,
        conn: &rusqlite::Connection,
        query: Vec<f32>,
        k: usize,
    ) -> Result<Vec<SearchResult>> {
        if k == 0 {
            return Ok(Vec::new());
        }

        // Get all vectors
        let mut stmt = conn.prepare("SELECT entity_id, vector, dimension FROM kg_vectors")?;

        let mut results = Vec::new();

        let rows = stmt.query_map([], |row| {
            let entity_id: i64 = row.get(0)?;
            let vector_blob: Vec<u8> = row.get(1)?;
            let dimension: i64 = row.get(2)?;

            // Deserialize vector
            let mut vector = Vec::with_capacity(dimension as usize);
            for chunk in vector_blob.chunks_exact(4) {
                let bytes: [u8; 4] = chunk.try_into().unwrap();
                vector.push(f32::from_le_bytes(bytes));
            }

            Ok((entity_id, vector))
        })?;

        for row in rows {
            let (entity_id, vector) = row?;

            // Calculate cosine similarity
            let similarity = cosine_similarity(&query, &vector);

            results.push(SearchResult {
                entity_id,
                similarity,
            });
        }

        // Sort by similarity (descending) and take top k
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());

        Ok(results.into_iter().take(k).collect())
    }

    /// Get vector for an entity.
    pub fn get_vector(&self, conn: &rusqlite::Connection, entity_id: i64) -> Result<Vec<f32>> {
        let mut stmt =
            conn.prepare("SELECT vector, dimension FROM kg_vectors WHERE entity_id = ?1")?;

        let (vector_blob, dimension): (Vec<u8>, i64) =
            stmt.query_row(params![entity_id], |row| Ok((row.get(0)?, row.get(1)?)))?;

        // Deserialize vector
        let mut vector = Vec::with_capacity(dimension as usize);
        for chunk in vector_blob.chunks_exact(4) {
            let bytes: [u8; 4] = chunk.try_into().unwrap();
            vector.push(f32::from_le_bytes(bytes));
        }

        Ok(vector)
    }

    /// Check if vectors exist and get their dimension.
    fn check_dimension(&self, conn: &rusqlite::Connection) -> Result<Option<usize>> {
        let mut stmt = conn.prepare("SELECT dimension FROM kg_vectors LIMIT 1")?;

        let dimension = stmt.query_row([], |row| {
            let dim: i64 = row.get(0)?;
            Ok(Some(dim as usize))
        });

        match dimension {
            Ok(dim) => Ok(dim),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Error::SQLite(e)),
        }
    }
}

/// Represents a search result from vector similarity search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub entity_id: i64,
    pub similarity: f32,
}

/// Calculate cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let mut dot_product = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    for i in 0..a.len() {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::entity::{insert_entity, Entity};
    use rusqlite::Connection;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);

        let c = vec![0.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &c);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_insert_vector() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity_id = insert_entity(&conn, &Entity::new("paper", "Test Paper")).unwrap();

        let store = VectorStore::new();
        let vector = vec![0.1, 0.2, 0.3, 0.4];

        store.insert_vector(&conn, entity_id, vector).unwrap();
    }

    #[test]
    fn test_search_vectors() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity1_id = insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();
        let entity2_id = insert_entity(&conn, &Entity::new("paper", "Paper 2")).unwrap();
        let entity3_id = insert_entity(&conn, &Entity::new("paper", "Paper 3")).unwrap();

        let store = VectorStore::new();
        let vector1 = vec![1.0, 0.0, 0.0];
        let vector2 = vec![0.0, 1.0, 0.0];
        let vector3 = vec![0.9, 0.1, 0.0];

        store.insert_vector(&conn, entity1_id, vector1).unwrap();
        store.insert_vector(&conn, entity2_id, vector2).unwrap();
        store.insert_vector(&conn, entity3_id, vector3).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = store.search_vectors(&conn, query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].entity_id, entity1_id);
        assert_eq!(results[1].entity_id, entity3_id);
    }

    #[test]
    fn test_invalid_dimension() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity_id = insert_entity(&conn, &Entity::new("paper", "Test Paper")).unwrap();

        let store = VectorStore::new();
        let vector1 = vec![0.1, 0.2, 0.3];
        let vector2 = vec![0.1, 0.2, 0.3, 0.4];

        store.insert_vector(&conn, entity_id, vector1).unwrap();

        let result = store.insert_vector(&conn, entity_id, vector2);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_insert() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        let entity1_id = insert_entity(&conn, &Entity::new("paper", "Paper 1")).unwrap();
        let entity2_id = insert_entity(&conn, &Entity::new("paper", "Paper 2")).unwrap();

        let store = VectorStore::new();
        let vectors = vec![
            (entity1_id, vec![0.1, 0.2, 0.3]),
            (entity2_id, vec![0.4, 0.5, 0.6]),
        ];

        store.insert_vectors_batch(&conn, vectors).unwrap();

        let query = vec![0.1, 0.2, 0.3];
        let results = store.search_vectors(&conn, query, 10).unwrap();
        assert_eq!(results.len(), 2);
    }
}
