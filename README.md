# SQLite Knowledge Graph

A Rust library for building and querying knowledge graphs using SQLite as the backend.

## Features

- **Entity Management**: Create, read, update, and delete typed entities with JSON properties
- **Relation Storage**: Define weighted relations between entities with graph traversal support
- **Vector Search**: Store embeddings and perform semantic search using cosine similarity
- **BFS Traversal**: Explore the graph with depth-limited breadth-first search
- **Transaction Support**: Batch operations with ACID guarantees
- **SQLite Native**: Full SQLite compatibility with bundling for portability

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
sqlite-knowledge-graph = "0.1"
```

## Quick Start

```rust
use sqlite_knowledge_graph::{KnowledgeGraph, Entity, Relation};

// Open or create a knowledge graph
let kg = KnowledgeGraph::open("knowledge.db")?;

// Create an entity with properties
let mut entity = Entity::new("paper", "Deep Learning Advances");
entity.set_property("author", serde_json::json!("Alice"));
entity.set_property("year", serde_json::json!(2024));
let paper_id = kg.insert_entity(&entity)?;

// Create a relation
let relation = Relation::new(paper_id, other_id, "cites", 0.8)?;
kg.insert_relation(&relation)?;

// Explore neighbors
let neighbors = kg.get_neighbors(paper_id, 2)?;
for neighbor in neighbors {
    println!("{} ({})", neighbor.entity.name, neighbor.relation.rel_type);
}

// Vector search for similar entities
let embedding = vec![0.1, 0.2, 0.3, ...];
kg.insert_vector(paper_id, embedding)?;
let results = kg.search_vectors(query_embedding, 10)?;
```

## API Overview

### KnowledgeGraph

The main entry point for the library.

```rust
pub struct KnowledgeGraph {
    // ...
}

impl KnowledgeGraph {
    // Create a new knowledge graph
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self>

    // Create an in-memory knowledge graph (for testing)
    pub fn open_in_memory() -> Result<Self>

    // Entity operations
    pub fn insert_entity(&self, entity: &Entity) -> Result<i64>
    pub fn get_entity(&self, id: i64) -> Result<Entity>
    pub fn list_entities(&self, entity_type: Option<&str>, limit: Option<i64>) -> Result<Vec<Entity>>
    pub fn update_entity(&self, entity: &Entity) -> Result<()>
    pub fn delete_entity(&self, id: i64) -> Result<()>

    // Relation operations
    pub fn insert_relation(&self, relation: &Relation) -> Result<i64>
    pub fn get_neighbors(&self, entity_id: i64, depth: u32) -> Result<Vec<Neighbor>>

    // Vector operations
    pub fn insert_vector(&self, entity_id: i64, vector: Vec<f32>) -> Result<()>
    pub fn search_vectors(&self, query: Vec<f32>, k: usize) -> Result<Vec<SearchResult>>

    // Transactions
    pub fn transaction(&self) -> Result<Transaction<'_>>
}
```

### Entity

Represents a typed entity in the knowledge graph.

```rust
pub struct Entity {
    pub id: Option<i64>,
    pub entity_type: String,
    pub name: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}
```

### Relation

Represents a weighted relation between entities.

```rust
pub struct Relation {
    pub id: Option<i64>,
    pub source_id: i64,
    pub target_id: i64,
    pub rel_type: String,
    pub weight: f64,  // 0.0 to 1.0
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: Option<i64>,
}
```

### VectorStore

Manages vector embeddings and similarity search.

```rust
pub struct SearchResult {
    pub entity_id: i64,
    pub similarity: f32,
}

// Utility function
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32
```

## Database Schema

The library creates the following tables:

### kg_entities

```sql
CREATE TABLE kg_entities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,
    name TEXT NOT NULL,
    properties TEXT,  -- JSON
    created_at INTEGER,
    updated_at INTEGER
);

CREATE INDEX idx_entities_type ON kg_entities(entity_type);
CREATE INDEX idx_entities_name ON kg_entities(name);
```

### kg_relations

```sql
CREATE TABLE kg_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    rel_type TEXT NOT NULL,
    weight REAL DEFAULT 1.0,
    properties TEXT,  -- JSON
    created_at INTEGER,
    FOREIGN KEY (source_id) REFERENCES kg_entities(id) ON DELETE CASCADE,
    FOREIGN KEY (target_id) REFERENCES kg_entities(id) ON DELETE CASCADE
);

CREATE INDEX idx_relations_source ON kg_relations(source_id);
CREATE INDEX idx_relations_target ON kg_relations(target_id);
CREATE INDEX idx_relations_type ON kg_relations(rel_type);
```

### kg_vectors

```sql
CREATE TABLE kg_vectors (
    entity_id INTEGER NOT NULL PRIMARY KEY,
    vector BLOB NOT NULL,
    dimension INTEGER NOT NULL,
    created_at INTEGER,
    FOREIGN KEY (entity_id) REFERENCES kg_entities(id) ON DELETE CASCADE
);
```

## Graph Traversal

The library supports BFS (Breadth-First Search) for exploring the graph:

```rust
// Get direct neighbors (depth 1)
let neighbors = kg.get_neighbors(entity_id, 1)?;

// Get neighbors up to depth 2
let neighbors = kg.get_neighbors(entity_id, 2)?;
```

Traversal features:
- Bidirectional (both incoming and outgoing relations)
- Depth-limited (max depth: 5)
- Cycle prevention
- No duplicate nodes in results

## Vector Search

Vectors are stored as BLOBs (32-bit floats, little-endian):

```rust
// Insert a vector embedding
let embedding: Vec<f32> = vec
![0.1, 0.2, 0.3, 0.4];
kg.insert_vector(entity_id, embedding)?;

// Search for similar entities
let query: Vec<f32> = vec
![0.2, 0.3, 0.4, 0.5];
let results = kg.search_vectors(query, 10)?;

for result in results {
    println!("Entity {}: {:.4}", result.entity_id, result.similarity);
}
```

Note: All vectors in the knowledge graph must have the same dimension.

## Transactions

For batch operations, use transactions for performance and consistency:

```rust
let tx = kg.transaction()?;

// Perform multiple operations
for i in 0..100 {
    let entity = Entity::new("item", format!("Item {}", i));
    tx.execute(
        "INSERT INTO kg_entities (entity_type, name) VALUES (?1, ?2)",
        [&entity.entity_type, &entity.name],
    )?;
}

tx.commit()?;
```

## Error Handling

The library uses a custom error type:

```rust
pub enum Error {
    SQLite(rusqlite::Error),
    Json(serde_json::Error),
    EntityNotFound(i64),
    RelationNotFound(i64),
    InvalidVectorDimension { expected: usize, actual: usize },
    InvalidDepth(u32),
    InvalidWeight(f64),
    InvalidEntityType(String),
    DatabaseClosed,
}
```

## SQL Custom Functions

The library registers custom SQL functions:

### kg_cosine_similarity

Calculate cosine similarity between two vectors:

```sql
SELECT kg_cosine_similarity(vector_blob1, vector_blob2);
```

Note: Full SQL function implementation is limited by the rusqlite API. For full functionality, use the Rust API directly.

## Testing

Run all tests:

```bash
cargo test
```

Run only integration tests:

```bash
cargo test --test integration
```

## Performance Considerations

- **Indexes**: The library creates indexes on commonly queried columns
- **Transactions**: Use transactions for batch operations
- **Vector dimension**: Lower dimensions are faster for search
- **Depth limit**: Keep graph traversal depth reasonable (≤ 3 for large graphs)

## Limitations

1. **Vector dimension**: All vectors must have the same dimension
2. **Traversal depth**: Maximum depth is 5
3. **SQL functions**: Limited SQL function support (use Rust API for full features)
4. **Single-threaded**: SQLite is single-threaded by default

## Future Enhancements

- [ ] Vector indexing for improved search performance
- [ ] Higher-order relations (n-ary relations)
- [ ] Path-finding algorithms
- [ ] Graph analytics (centrality, community detection)
- [ ] Graph visualization export
- [ ] Async API support

## License

MIT License

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## Acknowledgments

Built with:
- [rusqlite](https://github.com/rusqlite/rusqlite) - SQLite bindings
- [serde](https://serde.rs/) - Serialization framework
- [thiserror](https://docs.rs/thiserror/) - Error handling
