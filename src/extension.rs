//! SQLite extension entry point using sqlite-loadable
//!
//! This module provides the SQLite loadable extension interface.

use sqlite_loadable::{
    define_scalar_function, ext::sqlite3ext_result_text, prelude::*, Error, FunctionFlags,
};
use std::ffi::CString;
use std::os::raw::c_void;

// SQLite destructor constant - use SQLITE_TRANSIENT for SQLite to copy the string
extern "C" {
    fn sqlite3_destructor_type() -> Option<unsafe extern "C" fn(*mut c_void)>;
}

/// kg_version() - Returns the extension version
pub fn kg_version(
    context: *mut sqlite3_context,
    _values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    let version = CString::new(env!("CARGO_PKG_VERSION")).unwrap();
    unsafe {
        // SQLITE_TRANSIENT = -1, cast to destructor function pointer
        sqlite3ext_result_text(
            context,
            version.as_ptr(),
            version.as_bytes().len() as i32,
            Some(std::mem::transmute::<i64, unsafe extern "C" fn(*mut c_void)>(-1i64)),
        );
    }
    Ok(())
}

/// kg_stats() - Returns placeholder stats
pub fn kg_stats(
    context: *mut sqlite3_context,
    _values: &[*mut sqlite3_value],
) -> Result<(), Error> {
    let stats = CString::new("{\"message\": \"Extension loaded\"}").unwrap();
    unsafe {
        sqlite3ext_result_text(
            context,
            stats.as_ptr(),
            stats.as_bytes().len() as i32,
            Some(std::mem::transmute::<i64, unsafe extern "C" fn(*mut c_void)>(-1i64)),
        );
    }
    Ok(())
}

/// Register functions
fn register_functions(db: *mut sqlite3) -> Result<(), Error> {
    let flags = FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC;

    define_scalar_function(db, "kg_version", 0, kg_version, flags)?;
    define_scalar_function(db, "kg_stats", 0, kg_stats, flags)?;

    Ok(())
}

/// Extension entry point
#[sqlite_entrypoint]
pub fn sqlite3_sqlite_knowledge_graph_init(db: *mut sqlite3) -> Result<(), Error> {
    register_functions(db)
}
