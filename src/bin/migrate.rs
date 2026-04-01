//! CLI tool for data migration and RAG queries on the knowledge graph.

use sqlite_knowledge_graph::{Error, KnowledgeGraph};

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "migrate" => run_migrate(&args),
        "embed" => run_embed(&args),
        "search" => run_search(&args),
        "stats" => run_stats(&args),
        "context" => run_context(&args),
        "find-related" => run_find_related(&args),
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
    println!("  embed        Generate vector embeddings for entities");
    println!("  search       Semantic search with optional RAG context");
    println!("  stats        Show statistics about the knowledge graph");
    println!("  context      Get graph context for an entity");
    println!("  find-related Find entities related above a weight threshold");
    println!();
    println!("Migration command:");
    println!("  sqlite-kg migrate --source <knowledge.db> --skills <skills_dir> --target <kg.db>");
    println!();
    println!("Embed command:");
    println!("  sqlite-kg embed --db <kg.db> [--papers] [--skills] [--all]");
    println!();
    println!("Search command:");
    println!("  sqlite-kg search <query> --k <num> --db <kg.db>");
    println!();
    println!("Stats command:");
    println!("  sqlite-kg stats --db <kg.db>");
    println!();
    println!("Context command:");
    println!("  sqlite-kg context <entity_id> --depth <num> --db <kg.db>");
    println!();
    println!("Find-related command:");
    println!("  sqlite-kg find-related <entity_id> --threshold <0.0-1.0> --db <kg.db>");
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

fn run_embed(args: &[String]) -> Result<(), Error> {
    let mut db_path = "kg.db".to_string();
    let mut papers_only = false;
    let mut skills_only = false;
    let mut force = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                if i < args.len() {
                    db_path = args[i].clone();
                }
            }
            "--papers" => {
                papers_only = true;
            }
            "--skills" => {
                skills_only = true;
            }
            "--all" => {
                papers_only = false;
                skills_only = false;
            }
            "--force" => {
                force = true;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    println!("🔮 Starting embedding generation...");
    println!("  Database: {}", db_path);
    if force {
        println!("  Mode: force (regenerate all embeddings)");
    } else {
        println!("  Mode: incremental (skip entities with real embeddings)");
    }
    println!();

    // Check dependencies
    println!("Checking dependencies...");
    match sqlite_knowledge_graph::check_dependencies() {
        Ok(true) => {
            println!("✓ sentence-transformers is available");
        }
        Ok(false) => {
            println!("✗ sentence-transformers not found");
            println!();
            println!("To install required dependencies:");
            println!("  pip install sentence-transformers");
            println!();
            return Err(Error::Other(
                "sentence-transformers not installed. Run: pip install sentence-transformers"
                    .to_string(),
            ));
        }
        Err(e) => {
            println!("✗ Failed to check dependencies: {}", e);
            return Err(e);
        }
    }
    println!();

    // Open the knowledge graph
    let kg = KnowledgeGraph::open(&db_path)?;
    println!("✓ Opened knowledge graph database");
    println!();

    let generator = sqlite_knowledge_graph::EmbeddingGenerator::new().with_force(force);

    let stats = if papers_only {
        generator.generate_for_papers(kg.connection())?
    } else if skills_only {
        generator.generate_for_skills(kg.connection())?
    } else {
        generator.generate_for_all(kg.connection())?
    };

    println!();
    println!("✓ Embedding generation completed successfully!");
    println!();
    println!("Statistics:");
    println!("  Total entities: {}", stats.total_count);
    println!("  Processed:      {}", stats.processed_count);
    println!("  Skipped:        {}", stats.skipped_count);
    println!("  Dimension:      {}", stats.dimension);

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

    // Generate embedding for the query using sentence-transformers
    let generator = sqlite_knowledge_graph::EmbeddingGenerator::new();
    let embeddings = generator
        .generate_embeddings(vec![query.clone()])
        .map_err(|e| Error::Other(format!("Failed to generate query embedding: {}", e)))?;
    let embedding = embeddings
        .into_iter()
        .next()
        .unwrap_or_else(|| vec![0.0; 384]);

    println!("🔍 Searching for: {}", query);
    println!();

    if hybrid {
        let results = kg.kg_hybrid_search(&query, embedding, k)?;

        println!("Found {} results (hybrid search):", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!();
            println!(
                "{}. {} (similarity: {:.3})",
                idx + 1,
                result.entity.name,
                result.similarity
            );
            if let Some(context) = &result.context {
                println!("   Context: {} neighbors", context.neighbors.len());
            }
        }
    } else {
        let results = kg.kg_semantic_search(embedding, k)?;

        println!("Found {} results (semantic search):", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!();
            println!(
                "{}. {} (similarity: {:.3})",
                idx + 1,
                result.entity.name,
                result.similarity
            );

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
    let mut high_utility: Vec<_> = papers
        .iter()
        .filter_map(|p| {
            p.get_property("utility")
                .and_then(|v| v.as_f64())
                .map(|u| (p, u))
        })
        .collect();

    high_utility.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("Top 5 papers by utility:");
    for (idx, (paper, utility)) in high_utility.iter().take(5).enumerate() {
        println!("  {}. {} ({:.2})", idx + 1, paper.name, utility);
    }

    Ok(())
}

fn run_find_related(args: &[String]) -> Result<(), Error> {
    let mut entity_id: i64 = 0;
    let mut threshold: f64 = 0.5;
    let mut db_path = "kg.db".to_string();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--threshold" => {
                i += 1;
                if i < args.len() {
                    threshold = args[i].parse().unwrap_or(0.5);
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

    println!(
        "🔗 Related entities for Entity {} (threshold >= {:.2})",
        entity_id, threshold
    );
    println!();

    let results = kg.kg_find_related(entity_id, threshold)?;

    if results.is_empty() {
        println!("No related entities found above threshold {:.2}", threshold);
    } else {
        println!("Found {} related entities:", results.len());
        for (idx, (entity, weight)) in results.iter().enumerate() {
            println!(
                "  {}. [{}] {} (weight: {:.3})",
                idx + 1,
                entity.entity_type,
                entity.name,
                weight
            );
        }
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
        println!(
            "  {}. {} -> {} via {} (weight: {:.2})",
            idx + 1,
            neighbor.relation.source_id,
            neighbor.entity.name,
            neighbor.relation.rel_type,
            neighbor.relation.weight
        );
    }

    Ok(())
}
