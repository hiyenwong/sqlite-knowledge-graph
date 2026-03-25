//! SQLite extension entry point
//!
//! This module provides the SQLite loadable extension interface.

use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;

/// Register all knowledge graph functions
pub fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    // Graph statistics
    conn.create_scalar_function(
        "kg_stats",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_ctx| {
            Ok(r#"{"entities": 0, "relations": 0, "message": "Connect to kg.db first"}"#.to_string())
        },
    )?;

    // BFS traversal
    conn.create_scalar_function(
        "kg_bfs",
        2,
        FunctionFlags::SQLITE_UTF8,
        move |_ctx| {
            Ok("BFS traversal requires connection to knowledge graph database".to_string())
        },
    )?;

    // Shortest path
    conn.create_scalar_function(
        "kg_shortest_path",
        2,
        FunctionFlags::SQLITE_UTF8,
        move |_ctx| {
            Ok("Shortest path requires connection to knowledge graph database".to_string())
        },
    )?;

    // PageRank
    conn.create_scalar_function(
        "kg_pagerank",
        0,
        FunctionFlags::SQLITE_UTF8,
        move |_ctx| {
            Ok("PageRank requires connection to knowledge graph database".to_string())
        },
    )?;

    // Louvain communities
    conn.create_scalar_function(
        "kg_louvain",
        0,
        FunctionFlags::SQLITE_UTF8,
        move |_ctx| {
            Ok("Louvain requires connection to knowledge graph database".to_string())
        },
    )?;

    // Connected components
    conn.create_scalar_function(
        "kg_connected_components",
        0,
        FunctionFlags::SQLITE_UTF8,
        move |_ctx| {
            Ok("Connected components requires connection to knowledge graph database".to_string())
        },
    )?;

    // Semantic search (placeholder)
    conn.create_scalar_function(
        "kg_search",
        1,
        FunctionFlags::SQLITE_UTF8,
        move |ctx| {
            let query: String = ctx.get(0)?;
            Ok(format!("Search for '{}' requires vector embeddings", query))
        },
    )?;

    // Version info
    conn.create_scalar_function(
        "kg_version",
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_ctx| Ok(env!("CARGO_PKG_VERSION").to_string()),
    )?;

    Ok(())
}

// ============================================================================
// SQLite Extension Entry Points
// ============================================================================

/// Extension entry point for macOS (.dylib)
/// 
/// Usage:
/// ```bash
/// sqlite3 db.db ".load ./libsqlite_knowledge_graph"
/// sqlite3 db.db "SELECT kg_version();"
/// ```
#[cfg(target_os = "macos")]
#[no_mangle]
pub unsafe extern "C" fn sqlite3_sqliteknowledgegraph_init(
    db: *mut rusqlite::ffi::sqlite3,
    _pz_err_msg: *mut *mut std::os::raw::c_char,
    _p_api: *const rusqlite::ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    match Connection::from_handle(db) {
        Ok(conn) => match register_functions(&conn) {
            Ok(_) => {
                std::mem::forget(conn); // Keep connection alive
                rusqlite::ffi::SQLITE_OK
            }
            Err(e) => {
                eprintln!("Failed to register functions: {}", e);
                rusqlite::ffi::SQLITE_ERROR
            }
        },
        Err(e) => {
            eprintln!("Failed to create connection: {}", e);
            rusqlite::ffi::SQLITE_ERROR
        }
    }
}

/// Alternative entry point with underscores in name
#[cfg(target_os = "macos")]
#[no_mangle]
pub unsafe extern "C" fn sqlite3_sqlite_knowledge_graph_init(
    db: *mut rusqlite::ffi::sqlite3,
    pz_err_msg: *mut *mut std::os::raw::c_char,
    p_api: *const rusqlite::ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    sqlite3_sqliteknowledgegraph_init(db, pz_err_msg, p_api)
}

/// Extension entry point for Linux (.so)
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn sqlite3_sqliteknowledgegraph_init(
    db: *mut rusqlite::ffi::sqlite3,
    _pz_err_msg: *mut *mut std::os::raw::c_char,
    _p_api: *const rusqlite::ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
    match Connection::from_handle(db) {
        Ok(conn) => match register_functions(&conn) {
            Ok(_) => {
                std::mem::forget(conn);
                rusqlite::ffi::SQLITE_OK
            }
            Err(_) => rusqlite::ffi::SQLITE_ERROR,
        },
        Err(_) => rusqlite::ffi::SQLITE_ERROR,
    }
}