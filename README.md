# sqlite-knowledge-graph

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

SQLite knowledge graph plugin with vector search and RAG capabilities, written in Rust.

## Features

- 🧠 **Knowledge Graph** - Entity-Relation-Attribute model
- 🔍 **Vector Search** - Compatible with sqlite-vec format
- 🔄 **Hybrid RAG** - Combined vector + graph retrieval
- ⚡ **High Performance** - Native Rust implementation

## Installation

```bash
# Build from source
cargo build --release

# Load into SQLite
sqlite> .load ./target/release/libsqlite_knowledge_graph
```

## Usage

```sql
-- Create knowledge graph tables
SELECT kg_init();

-- Insert entity
SELECT kg_insert_entity('paper', 'Neural Networks Paper', '{"year": 2024}');

-- Insert relation
SELECT kg_insert_relation(1, 2, 'cites', 1.0);

-- Vector search
SELECT kg_vector_search('[0.1, 0.2, ...]', 10);

-- Hybrid RAG search
SELECT kg_rag_search('[0.1, 0.2, ...]', 'neural networks', 10);
```

## Documentation

- [Project Plan](PROJECT.md)
- [Development Log](DEVLOG.md)
- [API Reference](docs/API.md)

## License

MIT License - see [LICENSE](LICENSE) for details.