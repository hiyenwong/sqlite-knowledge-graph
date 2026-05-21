## ADDED Requirements

### Requirement: Compare two versions
The system SHALL compare two versions and return entities and relations that were added, removed, or common between them.

#### Scenario: Entities added in newer version
- **WHEN** version 1 has entities {A, B} and version 2 has entities {A, B, C}
- **AND** `version_compare` is called with v1=1, v2=2
- **THEN** the result includes `added_entities: [C]`, `removed_entities: []`, `common_entities: [A, B]`

#### Scenario: Entities removed in newer version
- **WHEN** version 1 has entities {A, B, C} and version 2 has entities {A, B}
- **AND** `version_compare` is called with v1=1, v2=2
- **THEN** the result includes `removed_entities: [C]`, `added_entities: []`

#### Scenario: Relations diff
- **WHEN** version 1 has relation R1 (A→B) and version 2 has relations R1 (A→B), R2 (B→C)
- **AND** `version_compare` is called with v1=1, v2=2
- **THEN** the result includes `added_relations: [R2]`, `removed_relations: []`, `common_relations: [R1]`

### Requirement: Version diff result structure
The system SHALL expose a `VersionDiff` struct with fields: `added_entities` (Vec<Entity>), `removed_entities` (Vec<Entity>), `common_entities` (Vec<Entity>), `added_relations` (Vec<Relation>), `removed_relations` (Vec<Relation>), `common_relations` (Vec<Relation>).

#### Scenario: Diff result is complete
- **WHEN** a diff is computed between two versions with different entity/relation sets
- **THEN** every entity and relation across both versions appears in exactly one of: added, removed, or common

### Requirement: Entity version history
The system SHALL return a list of all versions that contain a given entity.

#### Scenario: Entity exists in multiple versions
- **WHEN** entity has validity=0b101 (in versions 1 and 3)
- **AND** `version_entity_history` is called with that entity_id
- **THEN** the result is a list of Version structs for versions 1 and 3

#### Scenario: Unversioned entity
- **WHEN** entity has validity=NULL
- **AND** `version_entity_history` is called
- **THEN** the result is an empty list
