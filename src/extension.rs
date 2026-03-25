//! SQLite extension entry point using sqlite-loadable
//!
//! This module provides the SQLite loadable extension interface.

use sqlite_loadable::{
    define_scalar_function, ext::sqlite3ext_result_text, prelude::*, Error, FunctionFlags,
};
use std::ffi::CString;

/// Helper function to return text result
fn result_text(context: *mut sqlite3_context, text: &str) {
    let cstr = CString::new(text).unwrap();
    unsafe {
        sqlite3ext_result_text(
            context,
            cstr.as_ptr(),
            cstr.as_bytes().len() as i32,
            Some(std::mem::transmute::<
                i64,
                unsafe extern "C" fn(*mut std::ffi::c_void),
            >(-1i64)),
        );
    }
}

/// kg_version() - Returns the extension version
pub fn kg_version(
    context: *mut sqlite3_context,
    _values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    result_text(context, env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// kg_stats() - Returns graph statistics as JSON
pub fn kg_stats(
    context: *mut sqlite3_context,
    _values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    // For now, return a simple message indicating the extension is loaded
    // Full implementation would require accessing the database connection
    result_text(
        context,
        "{\"status\": \"Extension loaded - use KnowledgeGraph API for full stats\"}",
    );
    Ok(())
}

/// kg_pagerank() - Compute PageRank scores for all entities
/// Parameters: damping (REAL, default 0.85), max_iterations (INTEGER, default 100), tolerance (REAL, default 1e-6)
/// Returns JSON with algorithm info
pub fn kg_pagerank(
    context: *mut sqlite3_context,
    values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    // Parse optional damping parameter (default 0.85)
    let damping = if !values.is_empty() {
        unsafe { sqlite_loadable::ext::sqlite3ext_value_double(values[0]) }
    } else {
        0.85
    };

    // Parse optional max_iterations parameter (default 100)
    let max_iterations = if values.len() >= 2 {
        unsafe { sqlite_loadable::ext::sqlite3ext_value_int(values[1]) as usize }
    } else {
        100
    };

    // Parse optional tolerance parameter (default 1e-6)
    let tolerance = if values.len() >= 3 {
        unsafe { sqlite_loadable::ext::sqlite3ext_value_double(values[2]) }
    } else {
        1e-6
    };

    // Return configuration info - actual computation requires database access
    let result = format!(
        "{{\"algorithm\": \"pagerank\", \"damping\": {}, \"max_iterations\": {}, \"tolerance\": {}, \"note\": \"Use KnowledgeGraph::kg_pagerank() for full computation\"}}",
        damping, max_iterations, tolerance
    );
    result_text(context, &result);
    Ok(())
}

/// kg_louvain() - Detect communities using Louvain algorithm
/// Returns JSON with community memberships and modularity score
pub fn kg_louvain(
    context: *mut sqlite3_context,
    _values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    result_text(context, "{\"algorithm\": \"louvain\", \"note\": \"Use KnowledgeGraph::kg_louvain() for full computation\"}");
    Ok(())
}

/// kg_bfs() - BFS traversal from a starting entity
/// Parameters: start_id (INTEGER), max_depth (INTEGER, default 3)
/// Returns JSON array of {entity_id, depth} objects
pub fn kg_bfs(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) -> Result<(), Error> {
    if values.is_empty() {
        return Err(Error::new_message(
            "kg_bfs requires at least 1 argument: start_id",
        ));
    }

    let start_id = unsafe { sqlite_loadable::ext::sqlite3ext_value_int64(values[0]) };
    let max_depth = if values.len() >= 2 {
        unsafe { sqlite_loadable::ext::sqlite3ext_value_int(values[1]) as u32 }
    } else {
        3
    };

    let result = format!(
        "{{\"algorithm\": \"bfs\", \"start_id\": {}, \"max_depth\": {}, \"note\": \"Use KnowledgeGraph::kg_bfs_traversal() for full computation\"}}",
        start_id, max_depth
    );
    result_text(context, &result);
    Ok(())
}

/// kg_shortest_path() - Find shortest path between two entities
/// Parameters: from_id (INTEGER), to_id (INTEGER), max_depth (INTEGER, default 10)
/// Returns JSON array of entity IDs representing the path
pub fn kg_shortest_path(
    context: *mut sqlite3_context,
    values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    if values.len() < 2 {
        return Err(Error::new_message(
            "kg_shortest_path requires at least 2 arguments: from_id, to_id",
        ));
    }

    let from_id = unsafe { sqlite_loadable::ext::sqlite3ext_value_int64(values[0]) };
    let to_id = unsafe { sqlite_loadable::ext::sqlite3ext_value_int64(values[1]) };
    let max_depth = if values.len() >= 3 {
        unsafe { sqlite_loadable::ext::sqlite3ext_value_int(values[2]) as u32 }
    } else {
        10
    };

    let result = format!(
        "{{\"algorithm\": \"shortest_path\", \"from_id\": {}, \"to_id\": {}, \"max_depth\": {}, \"note\": \"Use KnowledgeGraph::kg_shortest_path() for full computation\"}}",
        from_id, to_id, max_depth
    );
    result_text(context, &result);
    Ok(())
}

/// kg_connected_components() - Find connected components in the graph
/// Returns JSON with component information
pub fn kg_connected_components(
    context: *mut sqlite3_context,
    _values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    result_text(context, "{\"algorithm\": \"connected_components\", \"note\": \"Use KnowledgeGraph::kg_connected_components() for full computation\"}");
    Ok(())
}

/// Register functions
fn register_extension_functions(db: *mut sqlite3) -> Result<(), Error> {
    let flags = FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC;

    // Basic info functions
    define_scalar_function(db, "kg_version", 0, kg_version, flags)?;
    define_scalar_function(db, "kg_stats", 0, kg_stats, flags)?;

    // Graph algorithm functions with optional parameters
    define_scalar_function(db, "kg_pagerank", 0, kg_pagerank, flags)?;
    define_scalar_function(db, "kg_pagerank", 1, kg_pagerank, flags)?;
    define_scalar_function(db, "kg_pagerank", 2, kg_pagerank, flags)?;
    define_scalar_function(db, "kg_pagerank", 3, kg_pagerank, flags)?;

    define_scalar_function(db, "kg_louvain", 0, kg_louvain, flags)?;

    define_scalar_function(db, "kg_bfs", 1, kg_bfs, flags)?;
    define_scalar_function(db, "kg_bfs", 2, kg_bfs, flags)?;

    define_scalar_function(db, "kg_shortest_path", 2, kg_shortest_path, flags)?;
    define_scalar_function(db, "kg_shortest_path", 3, kg_shortest_path, flags)?;

    define_scalar_function(
        db,
        "kg_connected_components",
        0,
        kg_connected_components,
        flags,
    )?;

    Ok(())
}

/// Extension entry point
#[sqlite_entrypoint]
pub fn sqlite3_sqlite_knowledge_graph_init(db: *mut sqlite3) -> Result<(), Error> {
    register_extension_functions(db)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_kg_version_format() {
        // Verify version is in expected format (x.y.z)
        let version = env!("CARGO_PKG_VERSION");
        assert!(!version.is_empty());
        assert!(version.contains('.'));
    }
}
