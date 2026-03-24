//! CLI tool for data migration and RAG queries on the knowledge graph.

use sqlite_knowledge_graph::{KnowledgeGraph, Error};

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "migrate" => run_migrate(&args),
        "search" => run_search(&args),
        "stats" => run_stats(&args),
        "context" => run_context(&args),
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("SQLite Knowledge Graph CLI");
    println!();
    println!("Usage: sqlite-kg <command> [options]");
    println!();
    println!("Commands:");
    println!("  migrate      Migrate data from Aerial's knowledge base");
    println!("  search       Semantic search with optional RAG context");
    println!("  stats        Show statistics about the knowledge graph");
    println!("  context      Get graph context for an entity");
    println!();
    println!("Migration command:");
    println!("  sqlite-kg migrate --source <knowledge.db> --skills <skills_dir> --target <kg.db>");
    println!();
    println!("Search command:");
    println!("  sqlite-kg search <query> --k <num> --db <kg.db>");
    println!();
    println!("Stats command:");
    println!("  sqlite-kg stats --db <kg.db>");
    println!();
    println!("Context command:");
    println!("  sqlite-kg context <entity_id> --depth <num> --db <kg.db>");
}

fn run_migrate(args: &[String]) -> Result<(), Error> {
    let mut source_db = String::new();
    let mut skills_dir = String::new();
    let mut target_db = "kg.db".to_string();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--source" => {
                i += 1;
                if i < args.len() {
                    source_db = args[i].clone();
                }
            }
            "--skills" => {
                i += 1;
                if i < args.len() {
                    skills_dir = args[i].clone();
                }
            }
            "--target" => {
                i += 1;
                if i < args.len() {
                    target_db = args[i].clone();
                }
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    if source_db.is_empty() || skills_dir.is_empty() {
        eprintln!("Error: --source and --skills are required");
        std::process::exit(1);
    }

    println!("🚀 Starting migration...");
    println!("  Source DB: {}", source_db);
    println!("  Skills dir: {}", skills_dir);
    println!("  Target DB: {}", target_db);
    println!();

    // Open or create the knowledge graph
    let kg = KnowledgeGraph::open(&target_db)?;
    println!("✓ Opened knowledge graph database");

    // Run full migration
    let stats = sqlite_knowledge_graph::migrate_all(&source_db, &skills_dir, &kg)?;

    println!();
    println!("✓ Migration completed successfully!");
    println!();
    println!("Statistics:");
    println!("  Papers migrated: {}", stats.papers_count);
    println!("  Skills migrated: {}", stats.skills_count);
    println!("  Relations built: {}", stats.relations_count);

    Ok(())
}

fn run_search(args: &[String]) -> Result<(), Error> {
    let mut query = String::new();
    let mut k: usize = 10;
    let mut db_path = "kg.db".to_string();
    let mut hybrid = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--k" => {
                i += 1;
                if i < args.len() {
                    k = args[i].parse().unwrap_or(10);
                }
            }
            "--db" => {
                i += 1;
                if i < args.len() {
                    db_path = args[i].clone();
                }
            }
            "--hybrid" => {
                hybrid = true;
            }
            _ if query.is_empty() => {
                query = args[i].clone();
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    if query.is_empty() {
        eprintln!("Error: query is required");
        std::process::exit(1);
    }

    let kg = KnowledgeGraph::open(&db_path)?;

    // Generate a random embedding for now (in real use, would use an embedding model)
    let embedding = generate_dummy_embedding();

    println!("🔍 Searching for: {}", query);
    println!();

    if hybrid {
        let results = kg.kg_hybrid_search(&query, embedding, k)?;

        println!("Found {} results (hybrid search):", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!();
            println!("{}. {} (similarity: {:.3})", idx + 1, result.entity.name, result.similarity);
            if let Some(context) = &result.context {
                println!("   Context: {} neighbors", context.neighbors.len());
            }
        }
    } else {
        let results = kg.kg_semantic_search(embedding, k)?;

        println!("Found {} results (semantic search):", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!();
            println!("{}. {} (similarity: {:.3})", idx + 1, result.entity.name, result.similarity);

            // Show some properties
            if let Some(arxiv_id) = result.entity.get_property("arxiv_id") {
                println!("   arxiv_id: {}", arxiv_id);
            }
            if let Some(utility) = result.entity.get_property("utility") {
                println!("   utility: {}", utility);
            }
        }
    }

    Ok(())
}

fn run_stats(args: &[String]) -> Result<(), Error> {
    let mut db_path = "kg.db".to_string();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                if i < args.len() {
                    db_path = args[i].clone();
                }
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let kg = KnowledgeGraph::open(&db_path)?;

    println!("📊 Knowledge Graph Statistics");
    println!();

    let all_entities = kg.list_entities(None, None)?;
    let papers = kg.list_entities(Some("paper"), None)?;
    let skills = kg.list_entities(Some("skill"), None)?;

    println!("Total entities: {}", all_entities.len());
    println!("  Papers: {}", papers.len());
    println!("  Skills: {}", skills.len());

    // Count relations (by checking a sample)
    let mut total_relations = 0;
    for paper in &papers {
        if let Some(id) = paper.id {
            let neighbors = kg.get_neighbors(id, 1)?;
            total_relations += neighbors.len();
        }
    }

    println!("Total relations: {}", total_relations);
    println!();

    // Show high utility papers
    let mut high_utility: Vec<_> = papers.iter().filter_map(|p| {
        p.get_property("utility")
            .and_then(|v| v.as_f64())
            .map(|u| (p, u))
    }).collect();

    high_utility.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("Top 5 papers by utility:");
    for (idx, (paper, utility)) in high_utility.iter().take(5).enumerate() {
        println!("  {}. {} ({:.2})", idx + 1, paper.name, utility);
    }

    Ok(())
}

fn run_context(args: &[String]) -> Result<(), Error> {
    let mut entity_id: i64 = 0;
    let mut depth: u32 = 1;
    let mut db_path = "kg.db".to_string();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--depth" => {
                i += 1;
                if i < args.len() {
                    depth = args[i].parse().unwrap_or(1);
                }
            }
            "--db" => {
                i += 1;
                if i < args.len() {
                    db_path = args[i].clone();
                }
            }
            _ if entity_id == 0 => {
                entity_id = args[i].parse().unwrap_or(0);
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    if entity_id == 0 {
        eprintln!("Error: entity_id is required");
        std::process::exit(1);
    }

    let kg = KnowledgeGraph::open(&db_path)?;

    println!("🔗 Graph Context for Entity {}", entity_id);
    println!();

    let context = kg.kg_get_context(entity_id, depth)?;

    println!("Root Entity:");
    println!("  ID: {:?}", context.root_entity.id);
    println!("  Type: {}", context.root_entity.entity_type);
    println!("  Name: {}", context.root_entity.name);
    println!();

    println!("Neighbors ({}):", context.neighbors.len());
    for (idx, neighbor) in context.neighbors.iter().enumerate() {
        println!("  {}. {} -> {} via {} (weight: {:.2})",
            idx + 1,
            neighbor.relation.source_id,
            neighbor.entity.name,
            neighbor.relation.rel_type,
            neighbor.relation.weight
        );
    }

    Ok(())
}

fn generate_dummy_embedding() -> Vec<f32> {
    // Generate a random embedding (in real use, would use an embedding model)
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u32;

    let mut rng = seed;
    (0..384).map(|_| {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (rng as f32) / (u32::MAX as f32)
    }).collect()
}
