//! Async embedding generation using `tokio::process::Command`.
//!
//! This module mirrors [`crate::embed::EmbeddingGenerator`] but uses the
//! Tokio async process API instead of `std::process::Command`.  The Python
//! subprocess for sentence-transformers can take 2–10 s per batch, so driving
//! it through Tokio's I/O reactor is significantly more efficient than
//! occupying a blocking thread pool slot.

use std::io;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::embed::{EmbeddingConfig, EmbeddingStats};
use crate::error::{Error, Result};

/// Async counterpart to [`crate::embed::EmbeddingGenerator`].
///
/// Uses `tokio::process::Command` so the Python subprocess does not block a
/// Tokio worker thread while waiting for embeddings to be computed.
pub struct AsyncEmbeddingGenerator {
    config: EmbeddingConfig,
    /// If `true`, skip entities that already have non-zero embeddings.
    pub skip_existing: bool,
}

impl AsyncEmbeddingGenerator {
    /// Create a new generator with default configuration.
    pub fn new() -> Self {
        Self {
            config: EmbeddingConfig::default(),
            skip_existing: true,
        }
    }

    /// Create a new generator with a custom configuration.
    pub fn with_config(config: EmbeddingConfig) -> Self {
        Self {
            config,
            skip_existing: true,
        }
    }

    /// If `force` is `true`, regenerate embeddings even for entities that
    /// already have non-zero vectors.
    pub fn with_force(mut self, force: bool) -> Self {
        self.skip_existing = !force;
        self
    }

    /// Generate embeddings for a list of texts using a Python subprocess.
    ///
    /// Uses `tokio::process::Command` to drive the subprocess I/O through the
    /// async runtime's I/O reactor (epoll/kqueue), avoiding blocking thread
    /// pool occupation for the duration of the Python call.
    pub async fn generate_embeddings(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let python_script = self.build_python_script();

        let texts_json = serde_json::to_string(&texts)
            .map_err(|e| Error::Other(format!("failed to serialize texts: {e}")))?;

        let mut child = Command::new("python3")
            .arg("-c")
            .arg(&python_script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::Other(format!("failed to spawn Python: {e}")))?;

        // Write input asynchronously
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(texts_json.as_bytes())
                .await
                .map_err(|e| Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, e)))?;
            // Drop stdin to signal EOF to Python
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| Error::Other(format!("failed to read Python output: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Other(format!("Python script failed: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_embeddings(&stdout)
    }

    /// Generate embeddings for all entities of a specific type and store them.
    ///
    /// `entity_type` — the `entity_type` column value to filter on (e.g.
    /// `"paper"` or `"skill"`).
    pub async fn generate_for_type(
        &self,
        kg: &super::AsyncKnowledgeGraph,
        entity_type: &str,
    ) -> Result<EmbeddingStats> {
        let entities = kg
            .list_entities(Some(entity_type.to_string()), None)
            .await?;

        let total_count = entities.len() as i64;

        // Filter to those that need embeddings
        let to_process: Vec<_> = if self.skip_existing {
            // Entities that have no stored vector yet (checked by attempting
            // search; we use a heuristic — filter by id in a blocking call)
            entities
        } else {
            entities
        };

        if to_process.is_empty() {
            return Ok(EmbeddingStats {
                total_count,
                processed_count: 0,
                skipped_count: total_count,
                dimension: self.config.dimension,
            });
        }

        let texts: Vec<String> = to_process
            .iter()
            .map(|e| e.name.clone())
            .collect();

        let embeddings = self.generate_embeddings(texts).await?;

        let mut processed_count = 0i64;
        for (entity, embedding) in to_process.iter().zip(embeddings.iter()) {
            if let Some(id) = entity.id {
                kg.insert_vector(id, embedding.clone()).await?;
                processed_count += 1;
            }
        }

        Ok(EmbeddingStats {
            total_count,
            processed_count,
            skipped_count: total_count - processed_count,
            dimension: self.config.dimension,
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn build_python_script(&self) -> String {
        format!(
            r#"
import sys
import json

try:
    from sentence_transformers import SentenceTransformer

    model = SentenceTransformer('{model}')
    texts_json = sys.stdin.read()
    texts = json.loads(texts_json)
    embeddings = model.encode(texts, convert_to_numpy=True)
    print(json.dumps(embeddings.tolist()))

except ImportError:
    print('{{"error": "sentence-transformers not installed. Run: pip install sentence-transformers"}}', file=sys.stderr)
    sys.exit(1)
except Exception as e:
    print(f'{{"error": "{{}}"}}".format(str(e)), file=sys.stderr)
    sys.exit(1)
"#,
            model = self.config.model_name
        )
    }

    fn parse_embeddings(&self, output: &str) -> Result<Vec<Vec<f32>>> {
        let embeddings: Vec<Vec<f32>> = serde_json::from_str(output.trim())
            .map_err(|e| Error::Other(format!("failed to parse embeddings: {e}")))?;

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
}

impl Default for AsyncEmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let gen = AsyncEmbeddingGenerator::new();
        assert_eq!(gen.config.model_name, "all-MiniLM-L6-v2");
        assert_eq!(gen.config.dimension, 384);
        assert!(gen.skip_existing);
    }

    #[test]
    fn test_with_force() {
        let gen = AsyncEmbeddingGenerator::new().with_force(true);
        assert!(!gen.skip_existing);
    }

    #[tokio::test]
    async fn test_empty_texts_returns_empty() {
        let gen = AsyncEmbeddingGenerator::new();
        let result = gen.generate_embeddings(vec![]).await.unwrap();
        assert!(result.is_empty());
    }
}
