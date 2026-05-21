## ADDED Requirements

### Requirement: Add entity to version
The system SHALL set the bit corresponding to a version in an entity's validity bitstring. If the entity's validity is NULL (unversioned), it SHALL be initialized to the version's bit using COALESCE.

#### Scenario: Add unversioned entity to first version
- **WHEN** entity has validity=NULL and `version_add_entity` is called with version_id=1
- **THEN** the entity's validity is set to `1 << 0` (0b1)

#### Scenario: Add entity to additional version
- **WHEN** entity has validity=0b01 (in version 1) and `version_add_entity` is called with version_id=2
- **THEN** the entity's validity is updated to 0b11 (in versions 1 and 2)

#### Scenario: Add to non-existent entity
- **WHEN** `version_add_entity` is called with an entity_id that does not exist
- **THEN** the system SHALL return an error

#### Scenario: Add to non-existent version
- **WHEN** `version_add_entity` is called with a version_id that does not exist
- **THEN** the system SHALL return an error

### Requirement: Remove entity from version
The system SHALL clear the bit corresponding to a version in an entity's validity bitstring. If the result would be 0, validity SHALL be set to NULL (returning to unversioned state).

#### Scenario: Remove entity from one of multiple versions
- **WHEN** entity has validity=0b11 (in versions 1,2) and `version_remove_entity` is called with version_id=1
- **THEN** the entity's validity becomes 0b10 (in version 2 only)

#### Scenario: Remove entity from its only version
- **WHEN** entity has validity=0b01 (in version 1 only) and `version_remove_entity` is called with version_id=1
- **THEN** the entity's validity is set to NULL (unversioned)

### Requirement: Add relation to version
The system SHALL set the bit corresponding to a version in a relation's validity bitstring, with the same NULL-handling semantics as entity.

#### Scenario: Add relation to version
- **WHEN** relation has validity=NULL and `version_add_relation` is called with version_id=1
- **THEN** the relation's validity is set to `1 << 0`

### Requirement: Remove relation from version
The system SHALL clear the bit corresponding to a version in a relation's validity bitstring, with the same zero-to-NULL semantics as entity.

#### Scenario: Remove relation from version
- **WHEN** relation has validity=0b11 and `version_remove_relation` is called with version_id=1
- **THEN** the relation's validity becomes 0b10

### Requirement: Bulk snapshot
The system SHALL support adding all current entities and/or relations to a version in a single transactional operation.

#### Scenario: Snapshot all entities into a version
- **WHEN** `version_snapshot_entities` is called with version_id=1
- **THEN** all entities with validity=NULL have their validity set to `1 << 0`, and all entities already in versioning have the bit OR'd in, within a single transaction
