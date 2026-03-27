//! Graph module for entity and relation storage.

pub mod entity;
pub mod hyperedge;
pub mod relation;
pub mod traversal;

pub use entity::{delete_entity, get_entity, insert_entity, list_entities, update_entity, Entity};
pub use hyperedge::{
    delete_hyperedge, get_entity_hyperedges, get_higher_order_neighbors, get_hyperedge,
    higher_order_bfs, higher_order_shortest_path, hyperedge_degree, hypergraph_entity_pagerank,
    insert_hyperedge, list_hyperedges, load_all_hyperedges, update_hyperedge, HigherOrderNeighbor,
    HigherOrderPath, HigherOrderPathStep, Hyperedge,
};
pub use relation::{get_neighbors, get_relations_by_source, insert_relation, Neighbor, Relation};
pub use traversal::{
    bfs_traversal, compute_graph_stats, dfs_traversal, find_shortest_path, Direction, GraphStats,
    PathStep, TraversalNode, TraversalPath, TraversalQuery,
};
