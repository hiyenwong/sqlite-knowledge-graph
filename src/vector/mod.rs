mod error;
pub use error::VectorError;

/// Vector storage configuration
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
}

#[derive(Debug, Clone, Copy)]
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

/// Vector search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: i64,
    pub score: f64,
}

/// Vector storage
pub struct VectorStore {
    config: VectorConfig,
    vectors: Vec<(i64, Vec<f32>)>,
}

impl VectorStore {
    pub fn new(config: VectorConfig) -> Self {
        Self {
            config,
            vectors: Vec::new(),
        }
    }
    
    pub fn insert(&mut self, id: i64, vector: Vec<f32>) -> Result<(), VectorError> {
        if vector.len() != self.config.dimension {
            return Err(VectorError::InvalidDimension {
                expected: self.config.dimension,
                actual: vector.len(),
            });
        }
        self.vectors.push((id, vector));
        Ok(())
    }
    
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = self.vectors
            .iter()
            .map(|(id, vec)| {
                let score = self.compute_distance(query, vec);
                SearchResult { id: *id, score }
            })
            .collect();
        
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(k);
        results
    }
    
    fn compute_distance(&self, a: &[f32], b: &[f32]) -> f64 {
        match self.config.metric {
            DistanceMetric::Cosine => {
                let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
                let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
                let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
                (dot / (norm_a * norm_b + 1e-10)) as f64
            }
            DistanceMetric::Euclidean => {
                let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
                (-sum.sqrt()) as f64
            }
            DistanceMetric::DotProduct => {
                a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>() as f64
            }
        }
    }
}