//! Integration tests using the Aerial knowledge backup data.

use rusqlite::Connection;
use sqlite_knowledge_graph::EmbeddingGenerator;
use sqlite_knowledge_graph::{Entity, KnowledgeGraph, Relation};

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
    assert!(!results.is_empty());
    println!("Vector search returned {} results", results.len());

    for result in results {
        println!(
            "Entity ID: {}, Similarity: {:.4}",
            result.entity_id, result.similarity
        );
    }
}

#[test]
fn test_embedding_generation() {
    // Check if sentence-transformers is available
    let check = std::process::Command::new("python3")
        .args(["-c", "import sentence_transformers"])
        .output();

    let python_available = match check {
        Ok(output) => output.status.success(),
        Err(_) => false,
    };

    if !python_available {
        println!("Skipping test_embedding_generation: sentence-transformers not installed");
        println!("To run this test, install: pip install sentence-transformers");
        return;
    }

    let conn = Connection::open_in_memory().unwrap();

    // Create the necessary table schema
    conn.execute(
        "CREATE TABLE kg_entities (
            id INTEGER PRIMARY KEY,
            entity_type TEXT NOT NULL,
            name TEXT NOT NULL
        )",
        [],
    )
    .unwrap();

    // Create the vectors table
    conn.execute(
        "CREATE TABLE kg_vectors (
            id INTEGER PRIMARY KEY,
            entity_id INTEGER NOT NULL,
            vector BLOB NOT NULL,
            FOREIGN KEY (entity_id) REFERENCES kg_entities(id)
        )",
        [],
    )
    .unwrap();

    let generator = EmbeddingGenerator::new();

    // Insert a mock entity into the database
    conn.execute(
        "INSERT INTO kg_entities (entity_type, name) VALUES ('paper', 'A Study on Embeddings')",
        [],
    )
    .unwrap();

    // Generate embeddings
    let embedding_stats = generator.generate_for_papers(&conn).unwrap();

    assert_eq!(embedding_stats.processed_count, 1);
    assert_eq!(embedding_stats.total_count, 1);

    println!("Embedding generation test passed");
}
