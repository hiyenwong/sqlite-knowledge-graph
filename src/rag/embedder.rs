//! Embedding abstraction for the RAG pipeline.
//!
//! Provides a trait-based interface so the engine is not coupled to any
//! specific embedding backend.  In production you'll typically wrap a
//! Python subprocess or an HTTP API; in tests you can use `FixedEmbedder`.

use crate::error::{Error, Result};
use std::io::{BufRead, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Trait for converting text into dense float vectors.
pub trait Embedder: Send + Sync {
    /// Embed a single text query, returning a normalised float vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

// ─────────────────────────────────────────────────────────────────────────────
// SubprocessEmbedder
// ─────────────────────────────────────────────────────────────────────────────

/// Embedder that talks to a long-lived Python subprocess over stdin/stdout.
///
/// The subprocess must implement a simple line protocol:
/// - stdin:  one text per line
/// - stdout: space-separated floats per line (same order)
///
/// Example Python server (`embed_server.py`):
/// ```python
/// import sys
/// from sentence_transformers import SentenceTransformer
/// model = SentenceTransformer("all-MiniLM-L6-v2")
/// for line in sys.stdin:
///     vec = model.encode(line.strip()).tolist()
///     print(" ".join(map(str, vec)), flush=True)
/// ```
pub struct SubprocessEmbedder {
    child: std::sync::Mutex<SubprocessState>,
}

struct SubprocessState {
    _child: Child,
    stdin: ChildStdin,
    stdout: std::io::BufReader<ChildStdout>,
}

impl SubprocessEmbedder {
    /// Spawn the subprocess.  `program` is e.g. `"python3"`,
    /// `args` is e.g. `&["embed_server.py"]`.
    pub fn new(program: &str, args: &[&str]) -> Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| Error::InvalidInput(format!("failed to spawn embedder: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::InvalidInput("no stdin handle".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::InvalidInput("no stdout handle".into()))?;

        Ok(Self {
            child: std::sync::Mutex::new(SubprocessState {
                _child: child,
                stdin,
                stdout: std::io::BufReader::new(stdout),
            }),
        })
    }
}

impl Embedder for SubprocessEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut state = self
            .child
            .lock()
            .map_err(|_| Error::InvalidInput("embedder mutex poisoned".into()))?;

        // Send the text (replace newlines so the protocol stays line-based)
        let sanitised = text.replace('\n', " ");
        writeln!(state.stdin, "{sanitised}")
            .map_err(|e| Error::InvalidInput(format!("write to embedder: {e}")))?;

        // Read one line of floats back
        let mut line = String::new();
        state
            .stdout
            .read_line(&mut line)
            .map_err(|e| Error::InvalidInput(format!("read from embedder: {e}")))?;

        line.split_whitespace()
            .map(|s| {
                s.parse::<f32>()
                    .map_err(|e| Error::InvalidInput(format!("bad float from embedder: {e}")))
            })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FixedEmbedder (testing)
// ─────────────────────────────────────────────────────────────────────────────

/// Deterministic embedder that always returns the same vector.
/// Useful in unit tests that need an `Embedder` but don't care about the values.
pub struct FixedEmbedder(pub Vec<f32>);

impl Embedder for FixedEmbedder {
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(self.0.clone())
    }
}
