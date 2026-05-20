# SmartVector: Self-Aware Vector Embeddings for sqlite-knowledge-graph

## Background

Based on arXiv:2604.20598 (SmartVector, April 2026), this spec defines adding temporal awareness, confidence decay, and relational dependency tracking to the existing knowledge graph system.

## Requirements

### Requirement: Temporal Awareness for Entities
The system SHALL track creation time, last access time, and validity windows for all entities and relations.

#### Scenario: Entity creation includes timestamp
GIVEN a new entity is inserted via `insert_entity`
WHEN the entity is stored
THEN the `created_at` field is automatically set to current Unix timestamp
AND `confidence` defaults to 1.0
AND `access_count` defaults to 0

#### Scenario: Entity access updates tracking
GIVEN an existing entity with id=123
WHEN `access_entity(123)` is called
THEN the entity's `access_count` is incremented by 1
AND `last_accessed` is updated to current timestamp

#### Scenario: Entity query filters by temporal range
GIVEN entities with various creation dates
WHEN `query_entities` is called with `created_after` and `created_before` filters
THEN only entities within the specified time range are returned

### Requirement: Confidence Decay Engine
The system SHALL implement a confidence scoring mechanism based on Ebbinghaus forgetting curve, access reinforcement, and feedback adjustment.

#### Scenario: Confidence decays over time
GIVEN an entity with initial confidence=1.0 created 30 days ago
WHEN `get_confidence(entity_id)` is called
THEN the returned confidence is < 1.0 (decayed by Ebbinghaus function)

#### Scenario: Access reinforces confidence
GIVEN an entity whose confidence has decayed to 0.5
WHEN `access_entity(entity_id)` is called
THEN the entity's confidence increases (reinforcement effect)

#### Scenario: Feedback adjusts confidence
GIVEN an entity with confidence=0.8
WHEN `update_confidence(entity_id, feedback=-0.2)` is called for negative feedback
THEN the entity's confidence is reduced to 0.6
AND the change is logged

#### Scenario: Confidence formula
The confidence function SHALL be:
```
confidence(t) = base * exp(-lambda * elapsed_days) 
              + access_bonus * log(1 + access_count) 
              + feedback_sum
```
Where:
- `base` = initial confidence (default 1.0)
- `lambda` = decay rate (default 0.05/day)
- `access_bonus` = reinforcement factor (default 0.1)
- `feedback_sum` = sum of all feedback adjustments (bounded to [-1, 1])

### Requirement: Dependency Graph & Ripple Propagation
The system SHALL track dependency relationships between entities and propagate updates along dependency edges.

#### Scenario: Dependency edges are stored
GIVEN entity A depends on entity B
WHEN `add_dependency(A_id, B_id, "supersedes")` is called
THEN a record is created in `kg_dependencies` table
AND the dependency type is stored

#### Scenario: Ripple propagation on update
GIVEN entity B has been superseded by entity A
AND entity C depends on entity B
WHEN entity B is updated or marked stale
THEN the RipplePropagator propagates a confidence penalty to entity C
AND the penalty is attenuated by hop distance (max depth = 2)

#### Scenario: Dependency types
The system SHALL support the following dependency types:
- `depends_on` — A requires B to be valid
- `depended_by` — B is required by A
- `supersedes` — A replaces B
- `contradicts` — A conflicts with B

### Requirement: Four-Signal Retrieval Scoring
The system SHALL replace pure cosine similarity with a four-signal scoring function for vector retrieval.

#### Scenario: Four-signal retrieval score
GIVEN a query vector and a corpus of entities with vectors
WHEN `retrieve_with_four_signal(query_vector, top_k)` is called
THEN each candidate is scored as:
```
final_score = w1 * cosine_similarity 
            + w2 * temporal_validity 
            + w3 * live_confidence 
            + w4 * graph_importance
```
WHERE:
- `cosine_similarity` = standard vector cosine similarity
- `temporal_validity` = 1.0 if within validity window, decaying otherwise
- `live_confidence` = current confidence score from ConfidenceEngine
- `graph_importance` = PageRank-style importance from dependency graph
- Default weights: w1=0.5, w2=0.2, w3=0.2, w4=0.1

#### Scenario: Configurable weights
GIVEN the retrieval system
WHEN `set_retrieval_weights(w1, w2, w3, w4)` is called
THEN subsequent retrievals use the new weights

### Requirement: Schema Migration v3
The system SHALL add new columns and tables via a non-destructive migration.

#### Scenario: Migration preserves existing data
GIVEN a database at schema version 2 with existing entities, relations, and vectors
WHEN `ensure_schema` is called on a library version that supports v3
THEN all existing data is preserved
AND new columns/tables are added
AND schema version is updated to 3

#### Scenario: New schema tables
Migration v3 SHALL create:
- `kg_dependencies` table: entity dependencies (source_id, target_id, dep_type, created_at)
- `kg_confidence_log` table: confidence change history (entity_id, old_value, new_value, reason, timestamp)
- New columns on `kg_entities`: `confidence`, `access_count`, `last_accessed`, `valid_from`, `valid_until`, `base_confidence`, `decay_rate`
- New columns on `kg_relations`: `confidence`, `valid_from`, `valid_until`

### Requirement: Performance & Benchmarking
The system SHALL maintain performance within acceptable bounds and provide benchmarking tools.

#### Scenario: Benchmark suite runs
GIVEN a test database with 10,000+ entities
WHEN `cargo bench` is run
THEN retrieval latency with four-signal scoring is within 2x of pure cosine retrieval
AND confidence computation is O(1) per entity (cached)
AND ripple propagation completes in < 100ms for graphs up to depth 2

#### Scenario: Memory overhead is bounded
GIVEN a database with 100,000 entities
WHEN SmartVector features are enabled
THEN memory overhead is < 10% compared to baseline
AND confidence cache uses < 50MB for 100K entities
