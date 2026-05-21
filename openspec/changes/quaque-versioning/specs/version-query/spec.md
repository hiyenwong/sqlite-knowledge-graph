## ADDED Requirements

### Requirement: Get entities for a version
The system SHALL return all entities whose validity bitstring has the bit set for the given version. Entities with validity=NULL SHALL be excluded from version-filtered queries.

#### Scenario: Query entities in a specific version
- **WHEN** `version_entities` is called with version_id=2 and entities {A: validity=0b11, B: validity=0b01, C: validity=NULL} exist
- **THEN** only entity A is returned (bit 1 set = version 2)

#### Scenario: Query entities with type filter
- **WHEN** `version_entities` is called with version_id=1 and entity_type="paper"
- **THEN** only entities of type "paper" with bit 0 set in validity are returned

### Requirement: Get relations for a version
The system SHALL return all relations whose validity bitstring has the bit set for the given version.

#### Scenario: Query relations in a specific version
- **WHEN** `version_relations` is called with version_id=1
- **THEN** only relations with `(validity & 1) != 0` are returned

#### Scenario: Filter relations by type in a version
- **WHEN** `version_relations` is called with version_id=1 and rel_type="cites"
- **THEN** only "cites" relations with bit 0 set are returned

### Requirement: Version-aware neighbor traversal
The system SHALL return neighbors of an entity filtered to a specific version — both the entity and the connecting relation MUST exist in that version.

#### Scenario: Get neighbors in version
- **WHEN** entity A (validity=0b11) has relations to B (relation validity=0b01) and C (relation validity=0b10)
- **AND** `version_neighbors` is called with entity A, version_id=1, depth=1
- **THEN** only neighbor B is returned (relation to B exists in version 1, relation to C does not)

#### Scenario: Neighbor entity not in version
- **WHEN** entity A (validity=0b11) has a relation to B (validity=0b10) via relation (validity=0b11)
- **AND** `version_neighbors` is called with version_id=1
- **THEN** B is excluded because B's validity bit 0 is not set (B doesn't exist in version 1)

### Requirement: Register kg_bit_count SQL function
The system SHALL register a `kg_bit_count(x)` SQLite scalar function that returns the population count of the integer argument, treating NULL as 0. It MUST be registered on both function surfaces: the rusqlite path (`functions::register_functions`, used by `KnowledgeGraph::open*`) and the loadable-extension path (`extension::register_extension_functions`, used by the `cdylib` `.load` entry point), so raw SQL can call it on either kind of connection.

#### Scenario: kg_bit_count available on a library connection
- **WHEN** SQL `SELECT kg_bit_count(7)` is executed on a `KnowledgeGraph` connection (7 = 0b111)
- **THEN** the result is 3

#### Scenario: kg_bit_count available in the loadable extension
- **WHEN** the compiled `cdylib` is `.load`-ed and `SELECT kg_bit_count(7)` is executed
- **THEN** the result is 3

#### Scenario: kg_bit_count with zero
- **WHEN** SQL `SELECT kg_bit_count(0)` is executed
- **THEN** the result is 0

#### Scenario: kg_bit_count with NULL
- **WHEN** SQL `SELECT kg_bit_count(NULL)` is executed
- **THEN** the result is 0
