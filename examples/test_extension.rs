//! Test loading sqlite-knowledge-graph extension

use rusqlite::Connection;

fn main() {
    println!("Testing sqlite-knowledge-graph extension...\n");

    let conn = Connection::open_in_memory().unwrap();

    // Load the extension
    let ext_path = "./target/release/libsqlite_knowledge_graph.dylib";
    println!("Loading extension from: {}", ext_path);

    unsafe {
        match conn.load_extension(ext_path, None) {
            Ok(_) => println!("✅ Extension loaded successfully!\n"),
            Err(e) => {
                println!("❌ Failed to load extension: {}\n", e);
                return;
            }
        }
    }

    // Test kg_version
    let version: String = conn.query_row("SELECT kg_version()", [], |row| row.get(0)).unwrap();
    println!("kg_version(): {}", version);

    // Test kg_stats
    let stats: String = conn.query_row("SELECT kg_stats()", [], |row| row.get(0)).unwrap();
    println!("kg_stats(): {}", stats);

    // Test kg_search
    let search: String = conn.query_row("SELECT kg_search('neural network')", [], |row| row.get(0)).unwrap();
    println!("kg_search('neural network'): {}", search);

    println!("\n✅ All tests passed!");
}