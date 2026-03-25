//! Graph module for entity and relation storage.

pub mod entity;
pub mod relation;
pub mod traversal;

pub use entity::{delete_entity, get_entity, insert_entity, list_entities, update_entity, Entity};
pub use relation::{get_neighbors, get_relations_by_source, insert_relation, Neighbor, Relation};
pub use traversal::{
    bfs_traversal, dfs_traversal, find_shortest_path, compute_graph_stats,
    TraversalNode, TraversalPath, PathStep, GraphStats, Direction, TraversalQuery,
};
