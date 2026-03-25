//! SQLite custom functions for knowledge graph operations.
//!
//! Note: Due to limitations in the rusqlite function context API (cannot access the connection
//! from scalar functions), the SQL functions are provided as a convenience layer. For full
//! functionality, use the KnowledgeGraph Rust API directly.

use crate::error::Result;

/// Register all knowledge graph custom functions with SQLite.
pub fn register_functions(conn: &rusqlite::Connection) -> Result<()> {
    // Register kg_cosine_similarity - utility function that can be called from SQL
    conn.create_scalar_function(
        "kg_cosine_similarity",
        2,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8,
        |ctx| {
            let vec1_blob: Vec<u8> = ctx.get(0)?;
            let vec2_blob: Vec<u8> = ctx.get(1)?;

            // Deserialize vectors from blobs
            let mut vec1 = Vec::new();
            for chunk in vec1_blob.chunks_exact(4) {
                let bytes: [u8; 4] = match chunk.try_into() {
                    Ok(b) => b,
                    Err(_) => return Ok(0.0f64),
                };
                vec1.push(f32::from_le_bytes(bytes));
            }

            let mut vec2 = Vec::new();
            for chunk in vec2_blob.chunks_exact(4) {
                let bytes: [u8; 4] = match chunk.try_into() {
                    Ok(b) => b,
                    Err(_) => return Ok(0.0f64),
                };
                vec2.push(f32::from_le_bytes(bytes));
            }

            if vec1.len() != vec2.len() {
                return Ok(0.0f64);
            }

            let mut dot_product = 0.0_f32;
            let mut norm_a = 0.0_f32;
            let mut norm_b = 0.0_f32;

            for i in 0..vec1.len() {
                dot_product += vec1[i] * vec2[i];
                norm_a += vec1[i] * vec1[i];
                norm_b += vec2[i] * vec2[i];
            }

            if norm_a == 0.0 || norm_b == 0.0 {
                return Ok(0.0f64);
            }

            let similarity = dot_product / (norm_a.sqrt() * norm_b.sqrt());
            Ok(similarity as f64)
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    #[test]
    fn test_register_functions() {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::create_schema(&conn).unwrap();

        // Verify registration succeeds
        assert!(register_functions(&conn).is_ok());

        // Test kg_cosine_similarity with identical vectors
        let mut vec1: Vec<u8> = Vec::new();
        vec1.extend_from_slice(&1.0_f32.to_le_bytes());
        vec1.extend_from_slice(&0.0_f32.to_le_bytes());
        vec1.extend_from_slice(&0.0_f32.to_le_bytes());
        let vec2 = vec1.clone();

        let sim: f64 = conn
            .query_row(
                "SELECT kg_cosine_similarity(?1, ?2)",
                params![vec1, vec2],
                |row| row.get(0),
            )
            .unwrap();
        assert!((sim - 1.0).abs() < 0.001);
    }
}
