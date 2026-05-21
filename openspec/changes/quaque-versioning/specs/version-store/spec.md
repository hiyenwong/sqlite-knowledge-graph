## ADDED Requirements

### Requirement: Create a named version
The system SHALL create a new version with a unique name, optional branch label, optional parent version, and optional description. The version ID SHALL be auto-incremented and returned to the caller.

#### Scenario: Create a version with all fields
- **WHEN** `create_version` is called with name="v1", branch="main", parent_id=None, description="initial snapshot"
- **THEN** a new row is inserted into `kg_versions` with those values, `is_merged=0`, `created_at` set to current unix timestamp, and the new version ID is returned

#### Scenario: Create a version with defaults
- **WHEN** `create_version` is called with name="v2" and no other arguments
- **THEN** a new row is inserted with branch="main", parent_id=NULL, description=NULL, is_merged=0

#### Scenario: Duplicate version name rejected
- **WHEN** `create_version` is called with a name that already exists in `kg_versions`
- **THEN** the system SHALL return an error

### Requirement: Delete a version
The system SHALL delete a version by ID. In a single transaction it SHALL clear that version's bit from the validity bitstring of every entity and relation, then remove the `kg_versions` row, freeing the version's `bit_slot` for reuse. Clearing a bit that leaves a validity of 0 SHALL collapse it back to NULL (the unversioned sentinel).

#### Scenario: Delete an existing version clears its bit
- **WHEN** `delete_version` is called with a valid version ID
- **THEN** the row is removed from `kg_versions`, the version's bit is cleared from all entity/relation validity bitstrings in the same transaction, and any row left with validity 0 is set to NULL

#### Scenario: Delete a non-existent version
- **WHEN** `delete_version` is called with an ID that does not exist
- **THEN** the system SHALL return `VersionNotFound`

### Requirement: Bit-slot allocation and concurrency limit
Each version SHALL own a `bit_slot` in the range `[0, 63]` that determines its bit position in the validity bitstring (`1 << bit_slot`). The system SHALL assign the lowest free slot at creation, drawn from slots not held by any live version, so that a slot freed by [Delete a version](#requirement-delete-a-version) is reused. The system SHALL support up to 64 concurrent versions.

#### Scenario: First version uses slot 0
- **WHEN** `create_version` is called on a database with no versions
- **THEN** the version is assigned `bit_slot=0` and its validity bit is `1 << 0`

#### Scenario: Slot reused after delete without leaking membership
- **WHEN** a version holding slot 0 (with member entities/relations) is deleted and a new version is created
- **THEN** the new version is assigned slot 0, and no entity or relation appears in the new version (the deleted version's bits were cleared on delete)

#### Scenario: Version limit exceeded
- **WHEN** `create_version` is called while all 64 slots are held by live versions
- **THEN** the system SHALL return `VersionLimitExceeded` (never panic or silently reuse a bit)

### Requirement: List versions
The system SHALL list all versions, optionally filtered by branch name.

#### Scenario: List all versions
- **WHEN** `list_versions` is called with no branch filter
- **THEN** all rows from `kg_versions` are returned, ordered by created_at descending

#### Scenario: List versions by branch
- **WHEN** `list_versions` is called with branch="feature-x"
- **THEN** only versions with branch="feature-x" are returned

### Requirement: Version struct
The system SHALL expose a `Version` struct with fields: `id` (i64), `name` (String), `branch` (String), `parent_id` (Option<i64>), `description` (Option<String>), `created_at` (Option<i64>), `is_merged` (bool).
