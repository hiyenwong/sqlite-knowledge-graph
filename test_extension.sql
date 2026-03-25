-- Test SQLite Extension Functions
-- Run with: sqlite3 <database> ".read test_extension.sql"

-- Note: Extension must be loaded first
-- SELECT load_extension('./target/release/libsqlite_knowledge_graph.dylib', 'sqlite3_sqlite_knowledge_graph_init');

-- Test kg_version()
SELECT '=== Testing kg_version() ===' as test;
SELECT kg_version() as version;

-- Test kg_stats()
SELECT '=== Testing kg_stats() ===' as test;
SELECT kg_stats() as stats;

-- Test kg_pagerank() with different parameter counts
SELECT '=== Testing kg_pagerank() ===' as test;
SELECT kg_pagerank() as pagerank_no_params;
SELECT kg_pagerank(0.85) as pagerank_damping;
SELECT kg_pagerank(0.85, 100) as pagerank_damping_iterations;
SELECT kg_pagerank(0.85, 100, 1e-6) as pagerank_full;

-- Test kg_louvain()
SELECT '=== Testing kg_louvain() ===' as test;
SELECT kg_louvain() as louvain;

-- Test kg_bfs() with different parameter counts
SELECT '=== Testing kg_bfs() ===' as test;
SELECT kg_bfs(1) as bfs_start_only;
SELECT kg_bfs(1, 3) as bfs_with_depth;

-- Test kg_shortest_path() with different parameter counts
SELECT '=== Testing kg_shortest_path() ===' as test;
SELECT kg_shortest_path(1, 5) as shortest_path_basic;
SELECT kg_shortest_path(1, 5, 10) as shortest_path_with_depth;

-- Test kg_connected_components()
SELECT '=== Testing kg_connected_components() ===' as test;
SELECT kg_connected_components() as components;

SELECT '=== All tests completed ===' as result;
