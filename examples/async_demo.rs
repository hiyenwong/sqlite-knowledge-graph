//! Async API demo: concurrent knowledge graph pipeline
//!
//! Simulates a research paper ingestion pipeline that:
//! 1. Concurrently inserts 20 papers + 5 skills
//! 2. Builds citation relations
//! 3. Stores synthetic vector embeddings
//! 4. Runs graph analytics (PageRank, Louvain)
//! 5. Performs semantic search
//! 6. Shows shortest path between two papers
//!
//! Run with:
//!   cargo run --example async_demo --features async

#![cfg(feature = "async")]

use std::sync::Arc;

use sqlite_knowledge_graph::{AsyncKnowledgeGraph, Direction, Entity, PageRankConfig, Relation};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Async Knowledge Graph Demo ===\n");

    let kg = Arc::new(AsyncKnowledgeGraph::open_in_memory_sync()?);

    // ── Step 1: Concurrent paper insertion ───────────────────────────────────
    println!("[1] Inserting 20 papers concurrently...");

    let paper_titles = vec![
        "Attention Is All You Need",
        "BERT: Pre-training of Deep Bidirectional Transformers",
        "GPT-3: Language Models are Few-Shot Learners",
        "ResNet: Deep Residual Learning for Image Recognition",
        "Word2Vec: Distributed Representations of Words",
        "GAN: Generative Adversarial Networks",
        "Transformer-XL: Attentive Language Models Beyond a Fixed-Length Context",
        "ELMo: Deep Contextualized Word Representations",
        "XLNet: Generalized Autoregressive Pretraining",
        "RoBERTa: A Robustly Optimized BERT Pretraining Approach",
        "ALBERT: A Lite BERT for Self-supervised Learning",
        "DistilBERT: a distilled version of BERT",
        "T5: Exploring the Limits of Transfer Learning",
        "BART: Denoising Sequence-to-Sequence Pre-training",
        "DeBERTa: Decoding-enhanced BERT with Disentangled Attention",
        "LLaMA: Open and Efficient Foundation Language Models",
        "Chinchilla: Training Compute-Optimal Large Language Models",
        "PaLM: Scaling Language Modeling with Pathways",
        "InstructGPT: Training language models to follow instructions",
        "RLHF: Learning to summarize from human feedback",
    ];

    let handles: Vec<_> = paper_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let kg = Arc::clone(&kg);
            let mut entity = Entity::new("paper", *title);
            entity.set_property("year", serde_json::json!(2017 + (i / 3) as i64));
            entity.set_property("citations", serde_json::json!(1000 - i as i64 * 40));
            tokio::spawn(async move { kg.insert_entity(entity).await })
        })
        .collect();

    let mut paper_ids = Vec::new();
    for h in handles {
        paper_ids.push(h.await??);
    }
    println!("    ✓ {} papers inserted", paper_ids.len());

    // ── Step 2: Insert skills sequentially ───────────────────────────────────
    println!("[2] Inserting skills...");
    let skills = ["NLP", "Computer Vision", "Reinforcement Learning", "Graph ML", "Generative AI"];
    let mut skill_ids = Vec::new();
    for skill in &skills {
        let id = kg.insert_entity(Entity::new("skill", *skill)).await?;
        skill_ids.push(id);
    }
    println!("    ✓ {} skills inserted", skill_ids.len());

    // ── Step 3: Build citation graph ─────────────────────────────────────────
    println!("[3] Building citation relations...");
    let citation_pairs = vec![
        (0, 1), (0, 4), (1, 4), (1, 5), (2, 0), (2, 1), (3, 4),
        (6, 0), (7, 1), (8, 0), (8, 1), (9, 1), (10, 1), (11, 1),
        (12, 0), (12, 1), (13, 0), (14, 1), (15, 0), (16, 0),
    ];

    for (from, to) in &citation_pairs {
        let rel = Relation::new(paper_ids[*from], paper_ids[*to], "cites", 0.9)?;
        kg.insert_relation(rel).await?;
    }

    // Link papers to skills
    let paper_skill_map = vec![(0, 0), (1, 0), (2, 0), (3, 1), (4, 0), (15, 4), (19, 2)];
    for (paper_idx, skill_idx) in &paper_skill_map {
        let rel = Relation::new(paper_ids[*paper_idx], skill_ids[*skill_idx], "uses_skill", 0.8)?;
        kg.insert_relation(rel).await?;
    }
    println!("    ✓ {} citation + {} skill relations", citation_pairs.len(), paper_skill_map.len());

    // ── Step 4: Store synthetic embeddings ───────────────────────────────────
    println!("[4] Storing synthetic 8-dim embeddings...");
    for (i, &id) in paper_ids.iter().enumerate() {
        // Synthetic embedding: papers in same "era" cluster together
        let era = (i / 5) as f32;
        let vec: Vec<f32> = (0..8)
            .map(|d| if d == (i % 8) { 1.0 } else { era * 0.1 })
            .collect();
        kg.insert_vector(id, vec).await?;
    }
    println!("    ✓ {} embeddings stored", paper_ids.len());

    // ── Step 5: Graph analytics ───────────────────────────────────────────────
    println!("[5] Running PageRank...");
    let pr_config = PageRankConfig {
        damping: 0.85,
        max_iterations: 100,
        tolerance: 1e-6,
    };
    let scores = kg.kg_pagerank(Some(pr_config)).await?;
    println!("    Top 5 by PageRank:");
    for (entity_id, score) in scores.iter().take(5) {
        let entity = kg.get_entity(*entity_id).await?;
        println!("      [{:.4}] {}", score, entity.name);
    }

    println!("[5b] Running Louvain community detection...");
    let communities = kg.kg_louvain().await?;
    println!("    {} communities detected, modularity = {:.4}", communities.num_communities, communities.modularity);

    // ── Step 6: Semantic search ───────────────────────────────────────────────
    println!("[6] Semantic search (query similar to paper index 0)...");
    let query_vec: Vec<f32> = (0..8).map(|d| if d == 0 { 1.0 } else { 0.0 }).collect();
    let results = kg.kg_semantic_search(query_vec, 3).await?;
    println!("    Top 3 semantic matches:");
    for r in &results {
        println!("      [{:.3}] {}", r.similarity, r.entity.name);
    }

    // ── Step 7: Graph traversal ───────────────────────────────────────────────
    println!("[7] BFS from '{}' (depth 2)...", paper_titles[2]);
    let bfs_nodes = kg.kg_bfs_traversal(paper_ids[2], Direction::Outgoing, 2).await?;
    println!("    {} nodes reachable:", bfs_nodes.len());
    for node in bfs_nodes.iter().take(5) {
        let e = kg.get_entity(node.entity_id).await?;
        println!("      depth={} | {}", node.depth, e.name);
    }

    // ── Step 8: Shortest path ─────────────────────────────────────────────────
    println!("[8] Shortest path: '{}' → '{}'", paper_titles[2], paper_titles[4]);
    match kg.kg_shortest_path(paper_ids[2], paper_ids[4], 5).await? {
        Some(path) => {
            println!("    Found path ({} hops, total weight = {:.2}):", path.steps.len(), path.total_weight);
            for step in &path.steps {
                let from = kg.get_entity(step.from_id).await?;
                let to = kg.get_entity(step.to_id).await?;
                println!("      {} --[{}]--> {}", from.name, step.relation_type, to.name);
            }
        }
        None => println!("    No path found within depth 5"),
    }

    // ── Step 9: Graph stats ───────────────────────────────────────────────────
    println!("[9] Graph statistics:");
    let stats = kg.kg_graph_stats().await?;
    println!("    Entities : {}", stats.total_entities);
    println!("    Relations: {}", stats.total_relations);
    println!("    Avg degree: {:.2}", stats.avg_degree);
    println!("    Density   : {:.4}", stats.density);

    println!("\n=== Demo complete ===");
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() {
    eprintln!("Run with: cargo run --example async_demo --features async");
}
