//! Simple test for extension loading

use rusqlite::Connection;

fn main() {
    println!("=== Testing sqlite-knowledge-graph extension ===\n");

    // Create in-memory database
    let conn = Connection::open_in_memory().expect("Failed to create in-memory DB");

    // Extension path
    let ext_path = std::env::current_dir()
        .expect("Failed to get current dir")
        .join("target/release/libsqlite_knowledge_graph.dylib");

    println!("Extension path: {:?}", ext_path);
    println!("Extension exists: {}\n", ext_path.exists());

    // Load extension
    unsafe {
        match conn.load_extension(&ext_path, None) {
            Ok(_) => {
                println!("✅ Extension loaded successfully!\n");

                // Test functions
                match conn.query_row("SELECT kg_version()", [], |row| row.get::<_, String>(0)) {
                    Ok(v) => println!("kg_version(): {}", v),
                    Err(e) => println!("❌ kg_version() failed: {}", e),
                }

                match conn.query_row("SELECT kg_stats()", [], |row| row.get::<_, String>(0)) {
                    Ok(v) => println!("kg_stats(): {}", v),
                    Err(e) => println!("❌ kg_stats() failed: {}", e),
                }
            }
            Err(e) => {
                println!("❌ Failed to load extension: {}", e);
            }
        }
    }
}
