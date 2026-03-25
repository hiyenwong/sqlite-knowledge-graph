//! Integration tests using the Aerial knowledge backup data.

use rusqlite::Connection;
use sqlite_knowledge_graph::{cosine_similarity, Entity, KnowledgeGraph, Relation};

#[test]
fn test_integration_with_aerial_backup() {
    // Open the backup database
    let backup_path = "~/.openclaw/workspace/knowledge/knowledge.db.backup.20260324";
    let backup_path_expanded = shellexpand::tilde(backup_path).to_string();

    if !std::path::Path::new(&backup_path_expanded).exists() {
        println!(
            "Warning: Backup database not found at {}",
            backup_path_expanded
        );
        println!("Skipping integration test.");
        return;
    }

    // Connect to the backup database
    let backup_conn = Connection::open(&backup_path_expanded).unwrap();

    // Query some data from the backup to verify structure
    let mut stmt = backup_conn.prepare("SELECT COUNT(*) FROM papers").unwrap();
    let paper_count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
    println!("Backup database contains {} papers", paper_count);

    // Create a new knowledge graph
    let kg = KnowledgeGraph::open_in_memory().unwrap();

    // Import some entities from the backup
    let mut stmt = backup_conn
        .prepare("SELECT arxiv_id, title, utility FROM papers LIMIT 5")
        .unwrap();

    let papers = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<f64>>(2)?,
            ))
        })
        .unwrap();

    let mut entity_ids = Vec::new();
    for paper in papers {
        let (arxiv_id, title, utility) = paper.unwrap();
        let title_for_print = title.clone();

        let mut entity = Entity::new("paper", title);
        entity.set_property("arxiv_id", serde_json::json!(arxiv_id));

        if let Some(util) = utility {
            entity.set_property("utility", serde_json::json!(util));
        }

        let id = kg.insert_entity(&entity).unwrap();
        entity_ids.push(id);
        println!("Imported paper: {} (ID: {})", title_for_print, id);
    }

    assert_eq!(entity_ids.len(), 5);

    // Create some relations between imported papers
    if entity_ids.len() >= 2 {
        let relation = Relation::new(entity_ids[0], entity_ids[1], "cites", 0.8).unwrap();
        kg.insert_relation(&relation).unwrap();

        let neighbors = kg.get_neighbors(entity_ids[0], 1).unwrap();
        assert_eq!(neighbors.len(), 1);
        println!(
            "Found {} neighbor(s) for entity {}",
            neighbors.len(),
            entity_ids[0]
        );
    }

    // Test vector operations with mock embeddings
    for (i, &entity_id) in entity_ids.iter().enumerate() {
        let vector = (0..10)
            .map(|j| ((i + 1) as f32) / ((j + 1) as f32))
            .collect::<Vec<_>>();
        kg.insert_vector(entity_id, vector).unwrap();
    }

    // Test vector search
    let query = vec![0.5; 10];
    let results = kg.search_vectors(query, 3).unwrap();
    assert!(results.len() <= 3);
    assert!(results.len() > 0);
    println!("Vector search returned {} results", results.len());

    for result in results {
        println!(
            "Entity ID: {}, Similarity: {:.4}",
            result.entity_id, result.similarity
        );
    }

    // Test transaction rollback
    {
        let tx = kg.transaction().unwrap();
        tx.execute(
            "INSERT INTO kg_entities (entity_type, name) VALUES ('test', 'Should Rollback')",
            [],
        )
        .unwrap();
        tx.rollback().unwrap();
    }

    let test_entities = kg.list_entities(Some("test"), None).unwrap();
    assert_eq!(test_entities.len(), 0);
    println!("Transaction rollback verified");

    println!("Integration test completed successfully!");
}

#[test]
fn test_cosine_similarity_edge_cases() {
    // Test with zero vector
    let vec1 = vec![0.0; 5];
    let vec2 = vec![1.0, 0.0, 0.0, 0.0, 0.0];
    let sim = cosine_similarity(&vec1, &vec2);
    assert_eq!(sim, 0.0);

    // Test with different lengths (should return 0.0)
    let vec1 = vec![1.0, 2.0, 3.0];
    let vec2 = vec![1.0, 2.0];
    let sim = cosine_similarity(&vec1, &vec2);
    assert_eq!(sim, 0.0);

    // Test with orthogonal vectors
    let vec1 = vec![1.0, 0.0, 0.0];
    let vec2 = vec![0.0, 1.0, 0.0];
    let sim = cosine_similarity(&vec1, &vec2);
    assert!((sim - 0.0).abs() < 0.001);

    // Test with opposite vectors
    let vec1 = vec![1.0, 1.0, 1.0];
    let vec2 = vec![-1.0, -1.0, -1.0];
    let sim = cosine_similarity(&vec1, &vec2);
    assert!((sim - (-1.0)).abs() < 0.001);
}

#[test]
fn test_batch_operations() {
    let kg = KnowledgeGraph::open_in_memory().unwrap();

    // Batch insert entities using a transaction
    {
        let tx = kg.transaction().unwrap();

        for i in 0..10 {
            let entity = Entity::new("paper", format!("Paper {}", i));
            tx.execute(
                "INSERT INTO kg_entities (entity_type, name, properties) VALUES (?1, ?2, '{}')",
                [&entity.entity_type, &entity.name],
            )
            .unwrap();
        }

        tx.commit().unwrap();
    }

    let entities = kg.list_entities(Some("paper"), None).unwrap();
    assert_eq!(entities.len(), 10);

    // Batch insert vectors
    let entity_ids: Vec<i64> = entities.iter().filter_map(|e| e.id).collect();

    for entity_id in &entity_ids {
        let vector = vec![1.0; 10];
        kg.insert_vector(*entity_id, vector).unwrap();
    }

    let query = vec![1.0; 10];
    let results = kg.search_vectors(query, 10).unwrap();
    assert_eq!(results.len(), 10);
}

#[test]
fn test_graph_traversal_complex() {
    let kg = KnowledgeGraph::open_in_memory().unwrap();

    // Create a more complex graph structure
    //     1 -> 2 -> 4
    //     |    |
    //     v    v
    //     3 -> 5

    let entity_ids: Vec<i64> = (0..5)
        .map(|i| {
            kg.insert_entity(&Entity::new("node", format!("Node {}", i)))
                .unwrap()
        })
        .collect();

    // Add relations
    let edges = [(0, 1), (0, 2), (1, 3), (1, 4), (2, 4)];

    for (from, to) in edges {
        let relation = Relation::new(entity_ids[from], entity_ids[to], "connects", 1.0).unwrap();
        kg.insert_relation(&relation).unwrap();
    }

    // Test different depths
    let depth0 = kg.get_neighbors(entity_ids[0], 0).unwrap();
    assert_eq!(depth0.len(), 0);

    let depth1 = kg.get_neighbors(entity_ids[0], 1).unwrap();
    assert_eq!(depth1.len(), 2);

    let depth2 = kg.get_neighbors(entity_ids[0], 2).unwrap();
    assert_eq!(depth2.len(), 4);

    let depth3 = kg.get_neighbors(entity_ids[0], 3).unwrap();
    assert_eq!(depth3.len(), 4); // No new nodes beyond depth 2

    // Verify no duplicates (visited nodes shouldn't be counted again)
    let depth1_names: Vec<&String> = depth1.iter().map(|n| &n.entity.name).collect();
    let depth2_names: Vec<&String> = depth2.iter().map(|n| &n.entity.name).collect();

    // depth2 should include depth1 nodes plus additional ones
    assert!(depth2_names.len() > depth1_names.len());
}

#[test]
fn test_entity_properties_complex() {
    let kg = KnowledgeGraph::open_in_memory().unwrap();

    // Create an entity with complex properties
    let mut entity = Entity::new("research_paper", "Deep Learning Advances");
    entity.set_property("authors", serde_json::json!(["Alice", "Bob"]));
    entity.set_property("year", serde_json::json!(2024));
    entity.set_property("citations", serde_json::json!(42));
    entity.set_property(
        "keywords",
        serde_json::json!(["machine learning", "neural networks", "AI"]),
    );
    entity.set_property(
        "metadata",
        serde_json::json!({
            "review_status": "accepted",
            "conference": "NeurIPS",
            "pages": [1, 2, 3, 4, 5]
        }),
    );

    let id = kg.insert_entity(&entity).unwrap();

    // Retrieve and verify properties
    let retrieved = kg.get_entity(id).unwrap();

    let expected_authors = serde_json::json!(["Alice", "Bob"]);
    assert_eq!(
        retrieved.get_property("authors").and_then(|v| v.as_array()),
        Some(expected_authors.as_array().unwrap())
    );

    assert_eq!(
        retrieved.get_property("year").and_then(|v| v.as_i64()),
        Some(2024)
    );

    assert_eq!(
        retrieved
            .get_property("metadata")
            .and_then(|v| v.as_object()),
        Some(
            serde_json::json!({
                "review_status": "accepted",
                "conference": "NeurIPS",
                "pages": [1, 2, 3, 4, 5]
            })
            .as_object()
            .unwrap()
        )
    );
}
