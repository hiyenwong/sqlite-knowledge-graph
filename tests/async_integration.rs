//! Integration tests for the async API.
//!
//! Compiled only when the `async` feature is enabled:
//!   cargo test --features async

#![cfg(feature = "async")]

use std::sync::Arc;

use sqlite_knowledge_graph::{
    graph::{Entity, Relation},
    AsyncKnowledgeGraph, Direction, KnowledgeGraph,
};

// ── Basic construction ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_open_in_memory() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();
    // Verify the inner lock is accessible
    let _inner = kg.inner();
}

// ── Entity CRUD ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_crud_entity() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    // Insert
    let entity = Entity::new("paper", "Async Test Paper");
    let id = kg.insert_entity(entity).await.unwrap();
    assert!(id > 0);

    // Read
    let retrieved = kg.get_entity(id).await.unwrap();
    assert_eq!(retrieved.name, "Async Test Paper");
    assert_eq!(retrieved.entity_type, "paper");

    // List
    let list = kg.list_entities(Some("paper".into()), None).await.unwrap();
    assert_eq!(list.len(), 1);

    // Update
    let mut updated = retrieved.clone();
    updated.name = "Updated Title".to_string();
    kg.update_entity(updated).await.unwrap();
    let after_update = kg.get_entity(id).await.unwrap();
    assert_eq!(after_update.name, "Updated Title");

    // Delete
    kg.delete_entity(id).await.unwrap();
    let after_delete = kg.get_entity(id).await;
    assert!(after_delete.is_err());
}

// ── Relation + neighbors ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_insert_relation_and_neighbors() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    let id_a = kg.insert_entity(Entity::new("node", "A")).await.unwrap();
    let id_b = kg.insert_entity(Entity::new("node", "B")).await.unwrap();
    let id_c = kg.insert_entity(Entity::new("node", "C")).await.unwrap();

    kg.insert_relation(Relation::new(id_a, id_b, "links_to", 0.9).unwrap())
        .await
        .unwrap();
    kg.insert_relation(Relation::new(id_b, id_c, "links_to", 0.8).unwrap())
        .await
        .unwrap();

    let neighbors_a = kg.get_neighbors(id_a, 1).await.unwrap();
    assert_eq!(neighbors_a.len(), 1);
    assert_eq!(neighbors_a[0].entity.id.unwrap(), id_b);

    // depth-2 from A should reach C as well
    let deep_neighbors = kg.get_neighbors(id_a, 2).await.unwrap();
    let ids: Vec<i64> = deep_neighbors
        .iter()
        .map(|n| n.entity.id.unwrap())
        .collect();
    assert!(ids.contains(&id_b));
    assert!(ids.contains(&id_c));
}

// ── Vector search ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_vector_search() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    let id = kg
        .insert_entity(Entity::new("item", "Vector Item"))
        .await
        .unwrap();

    let dim = 4usize;
    let vec_a = vec![1.0f32, 0.0, 0.0, 0.0];
    let vec_q = vec![1.0f32, 0.1, 0.0, 0.0];

    kg.insert_vector(id, vec_a).await.unwrap();

    let results = kg.search_vectors(vec_q, 1).await.unwrap();
    assert_eq!(results.len(), 1, "dim={dim}");
    assert_eq!(results[0].entity_id, id);
    assert!(results[0].similarity > 0.99);
}

// ── Graph traversal ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_bfs_traversal() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    let root = kg.insert_entity(Entity::new("n", "Root")).await.unwrap();
    let child1 = kg.insert_entity(Entity::new("n", "Child1")).await.unwrap();
    let child2 = kg.insert_entity(Entity::new("n", "Child2")).await.unwrap();

    kg.insert_relation(Relation::new(root, child1, "has_child", 1.0).unwrap())
        .await
        .unwrap();
    kg.insert_relation(Relation::new(root, child2, "has_child", 1.0).unwrap())
        .await
        .unwrap();

    let nodes = kg
        .kg_bfs_traversal(root, Direction::Outgoing, 1)
        .await
        .unwrap();

    let visited_ids: Vec<i64> = nodes.iter().map(|n| n.entity_id).collect();
    assert!(visited_ids.contains(&child1));
    assert!(visited_ids.contains(&child2));
}

#[tokio::test]
async fn test_async_shortest_path() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    let a = kg.insert_entity(Entity::new("n", "A")).await.unwrap();
    let b = kg.insert_entity(Entity::new("n", "B")).await.unwrap();
    let c = kg.insert_entity(Entity::new("n", "C")).await.unwrap();

    kg.insert_relation(Relation::new(a, b, "edge", 1.0).unwrap())
        .await
        .unwrap();
    kg.insert_relation(Relation::new(b, c, "edge", 1.0).unwrap())
        .await
        .unwrap();

    let path = kg.kg_shortest_path(a, c, 5).await.unwrap();
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path.start_id, a);
    assert_eq!(path.end_id, c);
    assert_eq!(path.steps.len(), 2);
}

// ── Graph algorithms ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_async_pagerank() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    let mut ids = Vec::new();
    for i in 0..5 {
        let name = format!("Node {i}");
        let id = kg.insert_entity(Entity::new("n", &name)).await.unwrap();
        ids.push(id);
    }

    // Create a simple ring
    for i in 0..5 {
        kg.insert_relation(Relation::new(ids[i], ids[(i + 1) % 5], "edge", 1.0).unwrap())
            .await
            .unwrap();
    }

    let scores = kg.kg_pagerank(None).await.unwrap();
    assert_eq!(scores.len(), 5);
    // All scores should sum to ~1.0
    let sum: f64 = scores.iter().map(|(_, s)| s).sum();
    assert!((sum - 1.0).abs() < 0.01, "PageRank sum={sum}");
}

#[tokio::test]
async fn test_async_graph_stats() {
    let kg = AsyncKnowledgeGraph::open_in_memory_sync().unwrap();

    let a = kg.insert_entity(Entity::new("n", "A")).await.unwrap();
    let b = kg.insert_entity(Entity::new("n", "B")).await.unwrap();
    kg.insert_relation(Relation::new(a, b, "edge", 1.0).unwrap())
        .await
        .unwrap();

    let stats = kg.kg_graph_stats().await.unwrap();
    assert_eq!(stats.total_entities, 2);
    assert_eq!(stats.total_relations, 1);
}

// ── Sync ↔ Async interop ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_sync_async_interop() {
    use tempfile::NamedTempFile;

    // Write via sync API
    let file = NamedTempFile::new().unwrap();
    let path = file.path().to_path_buf();
    {
        let sync_kg = KnowledgeGraph::open(&path).unwrap();
        let entity = Entity::new("doc", "Shared Doc");
        sync_kg.insert_entity(&entity).unwrap();
    }

    // Read via async API
    let async_kg = AsyncKnowledgeGraph::open_sync(&path).unwrap();
    let entities = async_kg
        .list_entities(Some("doc".into()), None)
        .await
        .unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].name, "Shared Doc");
}

#[tokio::test]
async fn test_into_async_conversion() {
    let sync_kg = KnowledgeGraph::open_in_memory().unwrap();

    // Insert via sync
    let entity = Entity::new("test", "Converted");
    sync_kg.insert_entity(&entity).unwrap();

    // Convert and read via async
    let async_kg = sync_kg.into_async();
    let entities = async_kg
        .list_entities(Some("test".into()), None)
        .await
        .unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].name, "Converted");
}

// ── Concurrency ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_inserts() {
    let kg = Arc::new(AsyncKnowledgeGraph::open_in_memory_sync().unwrap());

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let kg = Arc::clone(&kg);
            tokio::spawn(async move {
                let name = format!("Concurrent Entity {i}");
                kg.insert_entity(Entity::new("concurrent", &name)).await
            })
        })
        .collect();

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "concurrent insert failed: {result:?}");
    }

    let all = kg
        .list_entities(Some("concurrent".into()), None)
        .await
        .unwrap();
    assert_eq!(all.len(), 10);
}
