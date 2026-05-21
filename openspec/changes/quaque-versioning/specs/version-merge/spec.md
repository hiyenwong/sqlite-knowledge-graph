## ADDED Requirements

### Requirement: Merge versions with union strategy
The system SHALL create a new version that is the union of two or more source versions. For each entity/relation, if it exists in ANY source version, its validity bit for the new version SHALL be set.

#### Scenario: Union merge of two versions
- **WHEN** version 1 has entities {A, B} and version 2 has entities {B, C}
- **AND** `version_merge` is called with source_ids=[1,2], name="merged", strategy="union"
- **THEN** a new version 3 is created, and entities {A, B, C} all have bit 2 set in their validity

#### Scenario: Union merge of relations
- **WHEN** version 1 has relation R1 and version 2 has relations R1, R2
- **AND** union merge is performed
- **THEN** both R1 and R2 have the new version's bit set

### Requirement: Merge versions with intersection strategy
The system SHALL create a new version that is the intersection of two or more source versions. For each entity/relation, if it exists in ALL source versions, its validity bit for the new version SHALL be set.

#### Scenario: Intersection merge
- **WHEN** version 1 has entities {A, B, C} and version 2 has entities {B, C, D}
- **AND** `version_merge` is called with source_ids=[1,2], strategy="intersection"
- **THEN** a new version is created, and only entities {B, C} have the new version's bit set

### Requirement: Merge creates a new version row
The system SHALL create a new `kg_versions` row for the merged version. The `parent_id` of the merged version SHALL be set to the first source version's ID.

#### Scenario: Merge version metadata
- **WHEN** merge is called with source_ids=[1,2], name="merge-1-2"
- **THEN** a new kg_versions row is created with name="merge-1-2", parent_id=1, is_merged=1

### Requirement: Branch support via parent_id
The system SHALL support branching by creating a version with a parent_id that differs from the current branch. This establishes lineage but does not copy data — the new version starts empty.

#### Scenario: Create branch version
- **WHEN** `create_version` is called with name="feature-branch-v1", branch="feature", parent_id=5
- **THEN** a new version is created on branch "feature" with parent_id=5, and the version starts with no entities/relations assigned

### Requirement: Invalid merge detection
The system SHALL reject a merge with fewer than 2 source versions.

#### Scenario: Merge with one source
- **WHEN** `version_merge` is called with source_ids=[1]
- **THEN** the system SHALL return an error

#### Scenario: Merge with non-existent version
- **WHEN** `version_merge` is called with source_ids=[1, 999]
- **THEN** the system SHALL return an error
