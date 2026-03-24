# SQLite 知识图谱 Rust 插件技术调研报告

**调研日期：** 2026-03-24  
**调研人：** Tech Researcher

---

## 执行摘要

本报告针对开发 SQLite 知识图谱 Rust 插件进行了全面的技术调研。调研涵盖三个核心方向：
1. **sqlite-vec** - 向量搜索扩展的 API、数据格式和 Rust 集成方式
2. **Rust SQLite 扩展开发** - rusqlite vs sqlite-loadable 框架对比及实现方案
3. **知识图谱存储设计** - 实体-关系-属性模型、图遍历算法与向量搜索的结合

**核心结论：** 
- **推荐框架：** `sqlite-loadable` + `rusqlite` 组合方案
- **核心策略：** 将 sqlite-vec 作为底层向量引擎，使用虚拟表封装图操作，结合向量相似度和图遍历算法
- **适用场景：** 中小规模知识图谱（100-1000节点），本地优先、轻量级部署

---

## 1. sqlite-vec 技术调研

### 1.1 概述

**sqlite-vec** 是一个纯 C 编写的 SQLite 向量搜索扩展，支持 float、int8 和 binary 向量。它是 sqlite-vss 的继任者，可以在任何支持 SQLite 的平台运行（Linux/macOS/Windows、浏览器 WASM、Raspberry Pi 等）。

**关键特性：**
- 纯 C 实现，无外部依赖
- 支持多种向量类型（float、int8、binary）
- 通过 `vec0` 虚拟表提供 SQL 接口
- 可在构建时静态链接到 Rust 应用

### 1.2 API 和数据格式

#### 核心函数
```sql
-- 版本信息
SELECT vec_version();

-- 向量转 JSON（调试用）
SELECT vec_to_json(?);

-- 向量长度
SELECT vec_length(?);
```

#### vec0 虚拟表语法
```sql
CREATE VIRTUAL TABLE vec_table_name USING vec0(
    vector_embedding float[768],  -- 主向量列
    -- 可选列类型：
    metadata_column TEXT,         -- 元数据列（可参与 WHERE 过滤）
    partition_key INTEGER PARTITION KEY,  -- 分区键（索引分片）
    +auxiliary_column TEXT        -- 辅助列（不索引，仅存储）
);
```

#### 向量存储格式
- **float**: 每个浮点数占 4 字节（32-bit）
- **int8**: 每个整数占 1 字节（8-bit）
- **binary**: 原始二进制数据

**推荐序列化方式（Rust）：**
```rust
use zerocopy::AsBytes;

let vector: Vec<f32> = vec![0.1, 0.2, 0.3];
let vector_bytes = vector.as_bytes();  // 零拷贝转换
```

### 1.3 Rust 集成方案

#### 依赖配置
```toml
[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
sqlite-vec = "0.1"
zerocopy = "0.7"
```

#### 注册扩展
```rust
use sqlite_vec::sqlite3_vec_init;
use rusqlite::{Connection, ffi::sqlite3_auto_extension};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        // 注册 sqlite-vec 扩展
        sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const ()
        )));
    }

    let db = Connection::open_in_memory()?;

    // 验证安装
    let (vec_version, _): (String, String) = db.query_row(
        "SELECT vec_version(), vec_to_json(?)",
        &[vec![0.1f32, 0.2f32, 0.3f32].as_bytes()],
        |row| Ok((row.get(0)?, row.get(1)?))
    )?;

    println!("sqlite-vec version: {}", vec_version);
    Ok(())
}
```

### 1.4 向量存储和检索机制

#### 存储向量
```sql
-- 创建表
CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    title TEXT,
    content TEXT,
    embedding BLOB
);

-- 创建向量索引
CREATE VIRTUAL TABLE vss_documents USING vec0(embedding float[768]);

-- 插入数据
INSERT INTO documents (title, content, embedding)
VALUES ('Document 1', '...', ?);

INSERT INTO vss_documents (rowid, embedding)
VALUES (last_insert_rowid(), ?);
```

#### KNN 查询
```sql
-- 基础 KNN 查询
SELECT 
    d.title,
    d.content,
    v.distance
FROM vss_documents v
JOIN documents d ON v.rowid = d.id
WHERE v.embedding MATCH ?
ORDER BY v.distance
LIMIT 10;
```

#### 带过滤条件的查询
```sql
CREATE VIRTUAL TABLE vec_movies USING vec0(
    movie_id INTEGER PRIMARY KEY,
    synopsis_embedding float[1024],
    genre TEXT,
    num_reviews INTEGER,
    mean_rating FLOAT,
    contains_violence BOOLEAN
);

-- 复合查询：向量相似度 + 元数据过滤
SELECT *
FROM vec_movies
WHERE synopsis_embedding MATCH ?  -- 向量匹配
  AND k = 5                        -- 返回结果数
  AND genre = 'scifi'               -- 元数据过滤
  AND num_reviews BETWEEN 100 AND 500
  AND mean_rating > 3.5
  AND contains_violence = false;
```

### 1.5 分区键优化

对于大数据集，使用分区键可以显著提升查询性能：

```sql
CREATE VIRTUAL TABLE vec_documents USING vec0(
    document_id INTEGER PRIMARY KEY,
    user_id INTEGER PARTITION KEY,  -- 按用户分片
    contents_embedding float[1024]
);

-- 查询特定用户的文档
SELECT document_id, distance
FROM vec_documents
WHERE contents_embedding MATCH ?
  AND k = 20
  AND user_id = 123;  -- 约束到特定用户
```

**使用原则：**
- 每个分区键值应关联约数百个向量
- 避免过度分片（会导致性能下降）
- 最多支持 4 个分区键列

---

## 2. Rust SQLite 扩展开发框架对比

### 2.1 rusqlite

**特点：**
- 最广泛使用的 Rust SQLite 绑定库
- 提供类型安全的 API
- 支持自定义函数和虚拟表

**限制：**
- **仅支持嵌入式扩展**（只能被 Rust 应用使用）
- 无法生成可被 SQLite CLI 或其他语言动态加载的扩展
- 不支持 `.load` 指令加载

**适用场景：**
- Rust 独立应用
- 不需要跨语言共享扩展

**示例：自定义函数**
```rust
use rusqlite::{Connection, Result, functions::FunctionFlags};

fn main() -> Result<()> {
    let db = Connection::open_in_memory()?;

    // 注册标量函数
    db.create_scalar_function(
        "hello",
        0,
        FunctionFlags::SQLITE_UTF8,
        |ctx| Ok(format!("Hello, World!"))
    )?;

    Ok(())
}
```

### 2.2 sqlite-loadable

**特点：**
- 专为创建可动态加载的 SQLite 扩展设计
- API 与 rusqlite 类似，易于迁移
- 可生成 `.so`/`.dylib`/`.dll` 文件
- 支持被 SQLite CLI、Python sqlite3 模块等加载

**局限性：**
- 项目较新（v0.0.5），稳定性待验证
- 大量 unsafe 代码
- 尚未成熟到用于生产环境

**适用场景：**
- 需要创建通用 SQLite 扩展
- 跨语言共享扩展功能

**示例：虚拟表**
```rust
use sqlite_loadable::{define_virtual_table, table::{VTab, VTabCursor}, Result};

#[sqlite_entrypoint]
pub fn sqlite3_myextension_init(db: *mut sqlite3) -> Result<()> {
    define_virtual_table::<MyVTab>(db, "my_table", None)?;
    Ok(())
}

pub struct MyVTab;

impl VTab for MyVTab {
    fn connect(
        &mut self,
        _db: *mut sqlite3,
        _args: &[&[u8]],
        _is_create: bool,
    ) -> Result<(String, String)> {
        Ok((
            "CREATE TABLE x(column1 TEXT)".to_string(),
            // ... 初始化逻辑
        ))
    }

    fn best_index(&self, _info: *mut sqlite3_index_info) -> Result<()> {
        Ok(())
    }

    fn open(&mut self) -> Result<Box<dyn VTabCursor>> {
        Ok(Box::new(MyVTabCursor::new()))
    }
}

pub struct MyVTabCursor;

impl MyVTabCursor {
    fn new() -> Self {
        MyVTabCursor { /* 初始化 */ }
    }
}

impl VTabCursor for MyVTabCursor {
    fn filter(&mut self, _idx_num: c_int, _idx_str: Option<&str>, _args: &[*mut sqlite3_value]) -> Result<()> {
        Ok(())
    }

    fn next(&mut self) -> Result<()> {
        Ok(())
    }

    fn eof(&self) -> bool {
        false
    }

    fn column(&self, _ctx: &mut Context, _col: c_int) -> Result<()> {
        Ok(())
    }

    fn rowid(&self) -> Result<i64> {
        Ok(0)
    }
}
```

### 2.3 框架对比总结

| 特性 | rusqlite | sqlite-loadable |
|------|----------|-----------------|
| **可动态加载** | ❌ 否 | ✅ 是 |
| **跨语言支持** | ❌ 仅 Rust | ✅ 是 |
| **稳定性** | ✅ 成熟 | ⚠️ 实验性 |
| **API 质量** | ✅ 优秀 | ✅ 良好 |
| **安全性** | ✅ Safe Rust | ⚠️ 大量 Unsafe |
| **社区规模** | ✅ 大 | ⚠️ 小 |
| **学习曲线** | ✅ 平缓 | ✅ 平缓 |
| **生产可用** | ✅ 是 | ⚠️ 谨慎 |

### 2.4 技术选型建议

**推荐方案：混合使用**

```
┌─────────────────────────────────────┐
│   Rust 应用层                        │
├─────────────────────────────────────┤
│   rusqlite (应用数据库操作)         │
├─────────────────────────────────────┤
│   sqlite-loadable (扩展接口)        │
├─────────────────────────────────────┤
│   SQLite 核心                        │
├─────────────────────────────────────┤
│   sqlite-vec (向量搜索)             │
│   自定义知识图谱扩展                 │
└─────────────────────────────────────┘
```

**策略说明：**
1. **应用层**：使用 `rusqlite` 进行常规数据库操作（CRUD、事务等）
2. **扩展层**：使用 `sqlite-loadable` 创建可加载的向量搜索和图操作扩展
3. **核心层**：直接集成 `sqlite-vec` C 代码，避免重复造轮子

**理由：**
- 避免重新实现 sqlite-vec 的复杂逻辑
- 利用 rusqlite 的稳定性和安全性
- 通过 sqlite-loadable 提供跨语言兼容性

---

## 3. 知识图谱存储设计

### 3.1 实体-关系-属性模型

#### 模式设计

```sql
-- 实体表
CREATE TABLE entities (
    id INTEGER PRIMARY KEY,
    type TEXT NOT NULL,           -- 实体类型（如 Person, Organization）
    name TEXT NOT NULL,           -- 实体名称
    properties TEXT,              -- JSON 格式的属性
    vector_embedding BLOB,        -- 实体的向量表示
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 关系表
CREATE TABLE relationships (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL,    -- 源实体 ID
    target_id INTEGER NOT NULL,    -- 目标实体 ID
    relation_type TEXT NOT NULL,   -- 关系类型（如 WORKS_FOR, KNOWS）
    properties TEXT,              -- JSON 格式的属性
    weight REAL DEFAULT 1.0,       -- 关系权重
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (source_id) REFERENCES entities(id),
    FOREIGN KEY (target_id) REFERENCES entities(id),
    UNIQUE (source_id, target_id, relation_type)
);

-- 向量索引
CREATE VIRTUAL TABLE vec_entities USING vec0(
    entity_id INTEGER PRIMARY KEY,
    entity_embedding float[768],
    entity_type TEXT,
    +entity_properties TEXT
);

-- 索引优化
CREATE INDEX idx_relationships_source ON relationships(source_id);
CREATE INDEX idx_relationships_target ON relationships(target_id);
CREATE INDEX idx_entities_type ON entities(type);
```

#### 属性存储策略

实体和关系的属性以 JSON 格式存储，提供灵活性：

```json
{
    "age": 35,
    "email": "john@example.com",
    "skills": ["Rust", "Python", "Machine Learning"],
    "location": "San Francisco",
    "company": "Tech Corp"
}
```

#### 向量表示

每个实体应有语义向量表示，用于相似度搜索和 RAG 场景：

```rust
struct Entity {
    id: i64,
    name: String,
    entity_type: String,
    embedding: Vec<f32>,
    properties: serde_json::Value,
}
```

### 3.2 图遍历算法

#### 基础遍历：邻居查询

```sql
-- 查询实体的所有邻居
SELECT 
    e.id,
    e.type,
    e.name,
    r.relation_type,
    r.weight
FROM entities e
JOIN relationships r ON e.id = r.target_id
WHERE r.source_id = ?;

-- 双向邻居查询
SELECT 
    CASE 
        WHEN r.source_id = ? THEN r.target_id 
        ELSE r.source_id 
    END as neighbor_id,
    e.type,
    e.name,
    r.relation_type,
    r.weight
FROM relationships r
JOIN entities e ON 
    (e.id = r.target_id AND r.source_id = ?) OR
    (e.id = r.source_id AND r.target_id = ?);
```

#### 多跳遍历（递归 CTE）

```sql
-- 查询 N 跳邻居
WITH RECURSIVE neighbors(n, hop, path) AS (
    SELECT target_id, 1, id || ' -> ' || target_id
    FROM relationships
    WHERE source_id = ?
    
    UNION
    
    SELECT r.target_id, n.hop + 1, n.path || ' -> ' || r.target_id
    FROM neighbors n
    JOIN relationships r ON n.n = r.source_id
    WHERE n.hop < 3  -- 限制跳数
)
SELECT n.hop, e.name, e.type, n.path
FROM neighbors n
JOIN entities e ON n.n = e.id
ORDER BY n.hop, n.path;
```

#### 度中心性计算

```sql
-- 计算每个实体的度中心性
SELECT 
    id,
    name,
    type,
    (
        (SELECT COUNT(*) FROM relationships WHERE source_id = entities.id) +
        (SELECT COUNT(*) FROM relationships WHERE target_id = entities.id)
    ) as degree_centrality
FROM entities
ORDER BY degree_centrality DESC
LIMIT 10;
```

#### 最短路径（BFS 实现）

```sql
-- 查询两个实体之间的最短路径
WITH RECURSIVE shortest_path(
    current_id,
    path,
    depth
) AS (
    SELECT source_id, CAST(source_id AS TEXT), 0
    FROM relationships
    WHERE source_id = ?
    
    UNION ALL
    
    SELECT 
        CASE 
            WHEN r.source_id = sp.current_id THEN r.target_id 
            ELSE r.source_id 
        END,
        sp.path || ' -> ' || 
        CASE 
            WHEN r.source_id = sp.current_id THEN r.target_id 
            ELSE r.source_id 
        END,
        sp.depth + 1
    FROM shortest_path sp
    JOIN relationships r ON 
        r.source_id = sp.current_id OR r.target_id = sp.current_id
    WHERE sp.depth < 10  -- 防止无限循环
)
SELECT path, depth
FROM shortest_path
WHERE current_id = ?
ORDER BY depth
LIMIT 1;
```

### 3.3 与向量搜索的结合

#### 混合查询策略

结合图遍历和向量相似度，提供更智能的检索：

```sql
-- 场景 1：向量相似度 + 图过滤
-- 找到与查询向量相似的实体，并限制在特定关系网络中
WITH similar_entities AS (
    SELECT 
        rowid,
        distance
    FROM vec_entities
    WHERE entity_embedding MATCH ?
      AND k = 10
)
SELECT 
    e.id,
    e.name,
    e.type,
    se.distance,
    r.relation_type
FROM similar_entities se
JOIN entities e ON se.rowid = e.id
LEFT JOIN relationships r ON r.source_id = e.id OR r.target_id = e.id
WHERE e.type = 'Person'
ORDER BY se.distance
LIMIT 5;
```

#### GraphRAG 实现

参考 GraphRAG 模式，结合实体提取、关系建模和向量检索：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedRelation {
    entity1: String,
    relation: String,
    entity2: String,
    strength: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphEntity {
    id: i64,
    name: String,
    entity_type: String,
    embedding: Vec<f32>,
    centrality: f32,
}

impl GraphEntity {
    // 插入实体到数据库
    async fn insert(&self, db: &Connection) -> Result<i64> {
        db.execute(
            "INSERT INTO entities (type, name, properties, vector_embedding) 
             VALUES (?, ?, ?, ?)",
            params![
                &self.entity_type,
                &self.name,
                "{}",  // JSON properties
                &self.embedding.as_bytes(),
            ],
        )?;
        Ok(db.last_insert_rowid())
    }
    
    // 查询相似实体（向量搜索）
    async fn find_similar(
        &self, 
        db: &Connection, 
        limit: usize
    ) -> Result<Vec<GraphEntity>> {
        let mut stmt = db.prepare(
            "SELECT rowid, distance FROM vec_entities 
             WHERE entity_embedding MATCH ? AND k = ?"
        )?;
        
        let rows = stmt.query_map(
            params![self.embedding.as_bytes(), limit],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        )?;
        
        let mut entities = Vec::new();
        for row in rows {
            let (id, distance) = row?;
            let entity = Self::load_by_id(db, id).await?;
            entities.push(entity);
        }
        
        Ok(entities)
    }
    
    // 查询邻居（图遍历）
    async fn find_neighbors(&self, db: &Connection) -> Result<Vec<GraphEntity>> {
        let mut stmt = db.prepare(
            "SELECT e.id, e.name, e.type 
             FROM entities e
             JOIN relationships r ON 
                (r.source_id = ? AND e.id = r.target_id) OR
                (r.target_id = ? AND e.id = r.source_id)"
        )?;
        
        let rows = stmt.query_map(
            params![self.id, self.id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        )?;
        
        let mut neighbors = Vec::new();
        for row in rows {
            let (id, name, entity_type) = row?;
            neighbors.push(Self::load_by_id(db, id).await?);
        }
        
        Ok(neighbors)
    }
}

impl GraphEntity {
    async fn load_by_id(db: &Connection, id: i64) -> Result<Self> {
        db.query_row(
            "SELECT id, name, type, vector_embedding FROM entities WHERE id = ?",
            params![id],
            |row| {
                let embedding_bytes: Vec<u8> = row.get(3)?;
                let embedding: Vec<f32> = embedding_bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                    .collect();
                
                Ok(GraphEntity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    embedding,
                    centrality: 0.0,
                })
            },
        )
    }
}
```

#### 优先级排序（中心性 + 相似度）

```sql
-- 综合查询：按中心性和向量相似度排序
WITH 
entity_stats AS (
    SELECT 
        e.id,
        e.name,
        e.type,
        (
            (SELECT COUNT(*) FROM relationships WHERE source_id = e.id) +
            (SELECT COUNT(*) FROM relationships WHERE target_id = e.id)
        ) as degree,
        (
            SELECT AVG(weight) 
            FROM relationships 
            WHERE source_id = e.id OR target_id = e.id
        ) as avg_weight
    FROM entities e
),
similarity_scores AS (
    SELECT 
        rowid,
        distance
    FROM vec_entities
    WHERE entity_embedding MATCH ? AND k = 20
),
combined_scores AS (
    SELECT 
        es.id,
        es.name,
        es.type,
        es.degree,
        es.avg_weight,
        ss.distance,
        -- 综合评分：70% 相似度 + 30% 中心性
        (1.0 - ss.distance) * 0.7 + 
        (es.degree / (SELECT MAX(degree) FROM entity_stats)) * 0.3 
        as combined_score
    FROM entity_stats es
    JOIN similarity_scores ss ON es.id = ss.rowid
)
SELECT * 
FROM combined_scores
WHERE type = ?
ORDER BY combined_score DESC
LIMIT 10;
```

---

## 4. 核心代码示例

### 4.1 项目结构

```
sqlite-knowledge-graph/
├── Cargo.toml
├── src/
│   ├── main.rs              # 主程序入口
│   ├── lib.rs               # 库入口
│   ├── extension.rs         # SQLite 扩展定义
│   ├── entities.rs          # 实体操作
│   ├── relationships.rs     # 关系操作
│   ├── vector_search.rs     # 向量搜索
│   ├── graph_traversal.rs   # 图遍历
│   └── models.rs            # 数据模型
└── examples/
    ├── basic_usage.rs       # 基础用法示例
    └── graph_rag.rs         # GraphRAG 示例
```

### 4.2 Cargo.toml

```toml
[package]
name = "sqlite-knowledge-graph"
version = "0.1.0"
edition = "2021"

[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
sqlite-vec = "0.1"
sqlite-loadable = "0.0.5"
zerocopy = "0.7"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"

[lib]
name = "sqlite_kg"
crate-type = ["cdylib", "rlib"]

[[example]]
name = "basic_usage"
path = "examples/basic_usage.rs"

[[example]]
name = "graph_rag"
path = "examples/graph_rag.rs"
```

### 4.3 扩展定义

```rust
// src/extension.rs
use rusqlite::{Connection, Result};
use sqlite_vec::sqlite3_vec_init;
use std::ffi::CString;

/// 初始化知识图谱扩展
pub fn init_knowledge_graph_extension(db: &Connection) -> Result<()> {
    // 启用扩展加载
    db.execute("PRAGMA enable_load_extension = ON", [])?;
    
    // 注册 sqlite-vec
    unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const ()
        )));
    }
    
    // 创建核心表
    create_core_tables(db)?;
    
    // 创建向量索引
    create_vector_indexes(db)?;
    
    Ok(())
}

fn create_core_tables(db: &Connection) -> Result<()> {
    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS entities (
            id INTEGER PRIMARY KEY,
            type TEXT NOT NULL,
            name TEXT NOT NULL,
            properties TEXT,
            vector_embedding BLOB,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
        "#,
        [],
    )?;
    
    db.execute(
        r#"
        CREATE TABLE IF NOT EXISTS relationships (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL,
            target_id INTEGER NOT NULL,
            relation_type TEXT NOT NULL,
            properties TEXT,
            weight REAL DEFAULT 1.0,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (source_id) REFERENCES entities(id),
            FOREIGN KEY (target_id) REFERENCES entities(id),
            UNIQUE (source_id, target_id, relation_type)
        )
        "#,
        [],
    )?;
    
    // 索引
    db.execute("CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_id)", [])?;
    db.execute("CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_id)", [])?;
    db.execute("CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type)", [])?;
    
    Ok(())
}

fn create_vector_indexes(db: &Connection) -> Result<()> {
    db.execute(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_entities 
        USING vec0(
            entity_id INTEGER PRIMARY KEY,
            entity_embedding float[768],
            entity_type TEXT,
            +entity_properties TEXT
        )
        "#,
        [],
    )?;
    
    Ok(())
}

/// 创建实体
pub fn create_entity(
    db: &Connection,
    entity_type: &str,
    name: &str,
    embedding: &[f32],
    properties: Option<&serde_json::Value>,
) -> Result<i64> {
    let props_json = properties
        .map(|p| serde_json::to_string(p).unwrap())
        .unwrap_or_else(|| "{}".to_string());
    
    db.execute(
        "INSERT INTO entities (type, name, properties, vector_embedding) 
         VALUES (?, ?, ?, ?)",
        params![entity_type, name, props_json, embedding.as_bytes()],
    )?;
    
    let entity_id = db.last_insert_rowid();
    
    // 更新向量索引
    db.execute(
        "INSERT INTO vec_entities (rowid, entity_embedding, entity_type, entity_properties) 
         VALUES (?, ?, ?, ?)",
        params![entity_id, embedding.as_bytes(), entity_type, props_json],
    )?;
    
    Ok(entity_id)
}

/// 创建关系
pub fn create_relationship(
    db: &Connection,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
    weight: f32,
    properties: Option<&serde_json::Value>,
) -> Result<i64> {
    let props_json = properties
        .map(|p| serde_json::to_string(p).unwrap())
        .unwrap_or_else(|| "{}".to_string());
    
    db.execute(
        "INSERT INTO relationships (source_id, target_id, relation_type, weight, properties) 
         VALUES (?, ?, ?, ?, ?)",
        params![source_id, target_id, relation_type, weight, props_json],
    )?;
    
    Ok(db.last_insert_rowid())
}
```

### 4.4 向量搜索

```rust
// src/vector_search.rs
use rusqlite::{Connection, Result, params};

pub struct VectorSearch {
    db: Connection,
}

impl VectorSearch {
    pub fn new(db: Connection) -> Self {
        Self { db }
    }
    
    /// 查找最相似的实体
    pub fn find_similar_entities(
        &self,
        query_embedding: &[f32],
        limit: usize,
        entity_type: Option<&str>,
    ) -> Result<Vec<SimilarEntity>> {
        let type_filter = if let Some(t) = entity_type {
            format!("AND e.type = '{}'", t)
        } else {
            String::new()
        };
        
        let sql = format!(
            r#"
            SELECT 
                e.id,
                e.name,
                e.type,
                ve.distance
            FROM vec_entities ve
            JOIN entities e ON ve.rowid = e.id
            WHERE ve.entity_embedding MATCH ?
              AND k = ?
              {}
            ORDER BY ve.distance
            "#,
            type_filter
        );
        
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(
            params![query_embedding.as_bytes(), limit],
            |row| {
                Ok(SimilarEntity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    similarity: 1.0 - row.get::<_, f32>(3)?, // 转换为相似度
                })
            },
        )?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }
    
    /// 混合查询：向量相似度 + 图中心性
    pub fn hybrid_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        entity_type: Option<&str>,
        importance_weight: f32, // 0.0-1.0，中心性权重
    ) -> Result<Vec<HybridResult>> {
        let type_filter = if let Some(t) = entity_type {
            format!("AND e.type = '{}'", t)
        } else {
            String::new()
        };
        
        let sql = format!(
            r#"
            WITH 
            entity_centrality AS (
                SELECT 
                    id,
                    (
                        (SELECT COUNT(*) FROM relationships WHERE source_id = e.id) +
                        (SELECT COUNT(*) FROM relationships WHERE target_id = e.id)
                    ) as degree
                FROM entities e
            ),
            similarity_results AS (
                SELECT 
                    rowid,
                    distance
                FROM vec_entities
                WHERE entity_embedding MATCH ? AND k = ?
            )
            SELECT 
                e.id,
                e.name,
                e.type,
                ec.degree,
                sr.distance,
                ((1.0 - sr.distance) * (1.0 - ?) + 
                 (ec.degree::f32 / (SELECT MAX(degree) FROM entity_centrality)) * ?) 
                as combined_score
            FROM similarity_results sr
            JOIN entities e ON sr.rowid = e.id
            JOIN entity_centrality ec ON e.id = ec.id
            WHERE 1=1 {}
            ORDER BY combined_score DESC
            LIMIT ?
            "#,
            type_filter
        );
        
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(
            params![query_embedding.as_bytes(), limit * 2, importance_weight, importance_weight, limit],
            |row| {
                Ok(HybridResult {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    degree_centrality: row.get(3)?,
                    similarity: 1.0 - row.get::<_, f32>(4)?,
                    combined_score: row.get(5)?,
                })
            },
        )?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        
        Ok(results)
    }
}

#[derive(Debug)]
pub struct SimilarEntity {
    pub id: i64,
    pub name: String,
    pub entity_type: String,
    pub similarity: f32,
}

#[derive(Debug)]
pub struct HybridResult {
    pub id: i64,
    pub name: String,
    pub entity_type: String,
    pub degree_centrality: i32,
    pub similarity: f32,
    pub combined_score: f32,
}
```

### 4.5 图遍历

```rust
// src/graph_traversal.rs
use rusqlite::{Connection, Result, params};

pub struct GraphTraversal {
    db: Connection,
}

impl GraphTraversal {
    pub fn new(db: Connection) -> Self {
        Self { db }
    }
    
    /// 获取实体的邻居
    pub fn get_neighbors(&self, entity_id: i64) -> Result<Vec<Neighbor>> {
        let sql = r#"
            SELECT 
                CASE 
                    WHEN r.source_id = ? THEN r.target_id 
                    ELSE r.source_id 
                END as neighbor_id,
                e.type,
                e.name,
                r.relation_type,
                r.weight,
                r.properties
            FROM relationships r
            JOIN entities e ON 
                (e.id = r.target_id AND r.source_id = ?) OR
                (e.id = r.source_id AND r.target_id = ?)
        "#;
        
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map(
            params![entity_id, entity_id, entity_id],
            |row| {
                Ok(Neighbor {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    name: row.get(2)?,
                    relation_type: row.get(3)?,
                    weight: row.get(4)?,
                    properties: row.get::<_, Option<String>>(5)?
                        .map(|p| serde_json::from_str(&p).unwrap())
                        .unwrap_or(serde_json::Value::Null),
                })
            },
        )?;
        
        let mut neighbors = Vec::new();
        for row in rows {
            neighbors.push(row?);
        }
        
        Ok(neighbors)
    }
    
    /// 多跳遍历（BFS）
    pub fn bfs_traversal(
        &self,
        start_entity_id: i64,
        max_hops: usize,
    ) -> Result<Vec<TraversalNode>> {
        let sql = r#"
            WITH RECURSIVE neighbors(id, hop, path, relation_chain) AS (
                SELECT target_id, 1, id || ' -> ' || target_id, relation_type
                FROM relationships
                WHERE source_id = ?
                
                UNION
                
                SELECT 
                    r.target_id, 
                    n.hop + 1, 
                    n.path || ' -> ' || r.target_id,
                    n.relation_chain || ' -> ' || r.relation_type
                FROM neighbors n
                JOIN relationships r ON n.id = r.source_id
                WHERE n.hop < ?
            )
            SELECT 
                n.id,
                e.type,
                e.name,
                n.hop,
                n.path,
                n.relation_chain
            FROM neighbors n
            JOIN entities e ON n.id = e.id
            ORDER BY n.hop, n.path
        "#;
        
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map(
            params![start_entity_id, max_hops],
            |row| {
                Ok(TraversalNode {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    name: row.get(2)?,
                    hop: row.get(3)?,
                    path: row.get(4)?,
                    relation_chain: row.get(5)?,
                })
            },
        )?;
        
        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(row?);
        }
        
        Ok(nodes)
    }
    
    /// 计算度中心性
    pub fn calculate_degree_centrality(&self, top_n: usize) -> Result<Vec<CentralityScore>> {
        let sql = r#"
            SELECT 
                id,
                name,
                type,
                (
                    (SELECT COUNT(*) FROM relationships WHERE source_id = entities.id) +
                    (SELECT COUNT(*) FROM relationships WHERE target_id = entities.id)
                ) as degree
            FROM entities
            ORDER BY degree DESC
            LIMIT ?
        "#;
        
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map(
            params![top_n],
            |row| {
                Ok(CentralityScore {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    score: row.get(3)?,
                })
            },
        )?;
        
        let mut scores = Vec::new();
        for row in rows {
            scores.push(row?);
        }
        
        Ok(scores)
    }
}

#[derive(Debug)]
pub struct Neighbor {
    pub id: i64,
    pub entity_type: String,
    pub name: String,
    pub relation_type: String,
    pub weight: f32,
    pub properties: serde_json::Value,
}

#[derive(Debug)]
pub struct TraversalNode {
    pub id: i64,
    pub entity_type: String,
    pub name: String,
    pub hop: i64,
    pub path: String,
    pub relation_chain: String,
}

#[derive(Debug)]
pub struct CentralityScore {
    pub id: i64,
    pub name: String,
    pub entity_type: String,
    pub score: i64,
}
```

### 4.6 完整示例：GraphRAG

```rust
// examples/graph_rag.rs
use sqlite_kg::{extension, vector_search::VectorSearch, graph_traversal::GraphTraversal};
use rusqlite::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Connection::open_in_memory()?;
    
    // 初始化扩展
    extension::init_knowledge_graph_extension(&db)?;
    
    // 创建示例实体
    let tech_corp = extension::create_entity(
        &db,
        "Organization",
        "Tech Corp",
        &[0.1, 0.2, 0.3, /* ... */],
        Some(&serde_json::json!({
            "industry": "Technology",
            "employees": 1000
        })),
    )?;
    
    let john = extension::create_entity(
        &db,
        "Person",
        "John Doe",
        &[0.4, 0.5, 0.6, /* ... */],
        Some(&serde_json::json!({
            "age": 35,
            "skills": ["Rust", "Python"]
        })),
    )?;
    
    let jane = extension::create_entity(
        &db,
        "Person",
        "Jane Smith",
        &[0.7, 0.8, 0.9, /* ... */],
        Some(&serde_json::json!({
            "age": 32,
            "skills": ["Python", "Machine Learning"]
        })),
    )?;
    
    // 创建关系
    extension::create_relationship(&db, john, tech_corp, "WORKS_FOR", 1.0, None)?;
    extension::create_relationship(&db, jane, tech_corp, "WORKS_FOR", 1.0, None)?;
    extension::create_relationship(&db, john, jane, "KNOWS", 0.8, None)?;
    
    // 场景 1：向量搜索
    let query_embedding = vec![0.2, 0.3, 0.4, /* ... */];
    let vs = VectorSearch::new(db.clone());
    let similar_entities = vs.find_similar_entities(&query_embedding, 5, None)?;
    
    println!("=== 相似实体（向量搜索） ===");
    for entity in similar_entities {
        println!("{} ({}): {:.2}", entity.name, entity.entity_type, entity.similarity);
    }
    
    // 场景 2：图遍历
    let gt = GraphTraversal::new(db.clone());
    let neighbors = gt.get_neighbors(john)?;
    
    println!("\n=== John 的邻居 ===");
    for neighbor in neighbors {
        println!("{} -[{}]-> {}", neighbor.name, neighbor.relation_type, neighbor.weight);
    }
    
    // 场景 3：混合搜索（向量 + 中心性）
    let hybrid_results = vs.hybrid_search(&query_embedding, 3, Some("Person"), 0.3)?;
    
    println!("\n=== 混合搜索结果 ===");
    for result in hybrid_results {
        println!(
            "{} (中心性: {}, 相似度: {:.2}, 综合评分: {:.2})",
            result.name,
            result.degree_centrality,
            result.similarity,
            result.combined_score
        );
    }
    
    // 场景 4：多跳遍历
    let traversal = gt.bfs_traversal(john, 3)?;
    
    println!("\n=== 2 跳邻居 ===");
    for node in traversal.iter().filter(|n| n.hop <= 2) {
        println!("跳数 {}: {} - {}", node.hop, node.name, node.path);
    }
    
    Ok(())
}
```

---

## 5. 实现路线图

### 阶段 1：基础设施（Week 1-2）

**目标：** 建立项目骨架，集成核心依赖

**任务：**
1. ✅ 项目初始化
   - 创建 Cargo.toml，配置依赖
   - 设置项目目录结构
   - 配置构建脚本（build.rs）

2. ✅ 扩展集成
   - 集成 sqlite-vec 扩展
   - 实现扩展注册机制
   - 测试扩展加载

3. ✅ 数据库模式设计
   - 定义 entities 和 relationships 表结构
   - 创建 vec0 虚拟表
   - 添加必要的索引

**交付物：**
- 可编译的项目
- 基础测试套件
- 文档：架构设计

### 阶段 2：核心功能（Week 3-4）

**目标：** 实现实体、关系和向量搜索功能

**任务：**
1. 实体管理
   - CRUD 操作
   - 批量插入/更新
   - 属性管理（JSON）

2. 关系管理
   - 创建/删除关系
   - 批量操作
   - 权重管理

3. 向量搜索
   - KNN 查询
   - 元数据过滤
   - 相似度计算

4. 基础图操作
   - 邻居查询
   - 度中心性计算

**交付物：**
- 完整的实体/关系 API
- 向量搜索模块
- 单元测试覆盖率 > 80%

### 阶段 3：高级功能（Week 5-6）

**目标：** 实现图遍历算法和混合查询

**任务：**
1. 图遍历算法
   - BFS/DFS 实现
   - 最短路径
   - 多跳遍历

2. 混合查询
   - 向量相似度 + 图中心性
   - 动态权重调整
   - 结果排序和过滤

3. 性能优化
   - 查询优化
   - 索引策略
   - 批量操作优化

**交付物：**
- 图遍历模块
- 混合查询 API
- 性能测试报告

### 阶段 4：扩展和集成（Week 7-8）

**目标：** 支持动态扩展和跨语言使用

**任务：**
1. 可加载扩展
   - 使用 sqlite-loadable
   - 生成 .so/.dylib/.dll
   - 测试跨语言加载

2. 文档和示例
   - API 文档
   - 使用示例
   - 最佳实践指南

3. 社区集成
   - 发布到 crates.io
   - 创建 GitHub 仓库
   - 编写 README

**交付物：**
- 可加载扩展库
- 完整文档
- 公开发布的 crate

### 阶段 5：高级特性（Week 9-10）

**目标：** 添加高级图算法和优化

**任务：**
1. 高级图算法
   - PageRank
   - 社区检测
   - 影响力计算

2. 增量更新
   - 向量增量索引
   - 图增量更新
   - 事务支持

3. 可视化支持
   - 导出为 Graphviz
   - 导出为 D3.js JSON
   - 网络图生成

**交付物：**
- 高级图算法库
- 可视化工具
- 增量更新 API

### 阶段 6：测试和优化（Week 11-12）

**目标：** 全面测试和性能优化

**任务：**
1. 全面测试
   - 单元测试
   - 集成测试
   - 压力测试

2. 性能调优
   - 查询性能分析
   - 内存使用优化
   - 并发处理

3. 生产准备
   - 错误处理
   - 日志记录
   - 监控支持

**交付物：**
- 完整测试套件
- 性能基准报告
- 生产就绪的版本

---

## 6. 技术选型建议

### 6.1 核心技术栈

| 层次 | 技术选型 | 理由 |
|------|---------|------|
| **数据库核心** | SQLite | 轻量级、嵌入式、零配置 |
| **向量搜索** | sqlite-vec | 纯 C 实现、高性能、SQLite 原生集成 |
| **Rust 绑定** | rusqlite + sqlite-loadable | 成熟稳定 + 可扩展性 |
| **序列化** | serde + serde_json | Rust 生态标准、类型安全 |
| **向量处理** | zerocopy | 零拷贝、高效 |

### 6.2 为什么不选其他方案？

#### 为什么不用专用向量数据库（Pinecone/Weaviate/Milvus）？
- ❌ 需要额外基础设施和运维成本
- ❌ 不适合本地优先部署
- ✅ sqlite-vec 提供相似功能，零依赖

#### 为什么不用 Neo4j？
- ❌ 需要独立服务，增加复杂度
- ❌ 许可成本（企业版）
- ✅ SQLite 足够满足中小规模需求

#### 为什么不用纯 Python 实现？
- ❌ 性能瓶颈（向量搜索、图遍历）
- ❌ 缺乏类型安全
- ✅ Rust 提供高性能和安全性

### 6.3 关键设计决策

1. **向量维度固定为 768**
   - 兼容常见嵌入模型（如 GTE-base）
   - 平衡精度和性能

2. **JSON 存储属性**
   - 灵活性优于固定列
   - 便于扩展和迁移

3. **分区键用于多租户**
   - 支持用户级隔离
   - 提升查询性能

4. **混合查询权重可配置**
   - 适应不同场景
   - 平衡相似度和重要性

---

## 7. 风险和挑战

### 7.1 技术风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| **sqlite-loadable 不稳定** | 高 | 暂时使用，等待成熟；考虑 C 扩展备选方案 |
| **性能瓶颈** | 中 | 提前进行压力测试；优化查询和索引 |
| **向量维度膨胀** | 中 | 支持多种向量维度；优化存储格式 |

### 7.2 工程挑战

1. **并发处理**
   - SQLite 默认单写入连接
   - 需要合理设计并发策略

2. **内存管理**
   - 大向量集可能占用大量内存
   - 考虑分批处理和流式查询

3. **向后兼容性**
   - 扩展 API 可能变化
   - 版本控制和迁移策略

---

## 8. 未来扩展方向

### 8.1 短期（6 个月）

- ✅ 支持更多向量类型（int8, binary）
- ✅ 实现缓存层（Redis 集成）
- ✅ 添加 GraphQL 接口
- ✅ 支持时间序列图

### 8.2 中期（1 年）

- 分布式部署（SQLite-WASM）
- 实时更新和通知
- 多模态向量（图像、音频）
- 与 LLM 深度集成

### 8.3 长期（2 年+）

- 自定义硬件加速
- 图神经网络支持
- 知识图谱嵌入学习
- 自动关系发现

---

## 9. 参考资源

### 9.1 官方文档

- **sqlite-vec**: https://github.com/asg017/sqlite-vec
- **rusqlite**: https://docs.rs/rusqlite/
- **sqlite-loadable**: https://docs.rs/sqlite-loadable/
- **SQLite**: https://www.sqlite.org/docs.html

### 9.2 教程和示例

- Using sqlite-vec in Rust: https://alexgarcia.xyz/sqlite-vec/rust.html
- How to use sqlite-vec: https://dev.to/stephenc222/how-to-use-sqlite-vec-to-store-and-query-vector-embeddings-58mf
- Building SQLite Extensions in Rust: https://blog.davidvassallo.me/2024/04/01/writing-sqlite-extensions-in-rust/
- Building a SQLite extension with Rust: https://www.seachess.net/notes/rust-sqlite-extension/

### 9.3 GraphRAG 参考

- Lightweight GraphRAG with SQLite: https://dev.to/stephenc222/how-to-build-lightweight-graphrag-with-sqlite-53le
- Knowledge GraphRAG with SQLite: https://github.com/KHAEntertainment/graphrag-with-sqlite_vec-ts-vercel-ai-sdk

### 9.4 学术参考

- TransE 模型（知识图谱嵌入）
- Graph Neural Networks
- Retrieval-Augmented Generation (RAG)

---

## 10. 总结

本调研证明了使用 SQLite 构建知识图谱系统的可行性，特别是在向量搜索和图遍历结合的场景下。sqlite-vec 提供了强大的向量搜索能力，而 rusqlite 和 sqlite-loadable 提供了灵活的 Rust 扩展开发框架。

**关键优势：**
- ✅ 轻量级、零配置
- ✅ 高性能向量搜索
- ✅ 灵活的图遍历
- ✅ Rust 类型安全
- ✅ 易于部署和扩展

**适用场景：**
- 中小规模知识图谱（100-1000 节点）
- 本地优先的应用
- 需要 GraphRAG 能力的系统
- 对性能和资源敏感的环境

**下一步行动：**
1. 按照 12 周路线图推进开发
2. 优先实现核心 CRUD 和向量搜索
3. 早期引入 GraphRAG 场景验证
4. 持续关注 sqlite-loadable 的进展

---

**报告版本：** v1.0  
**最后更新：** 2026-03-24  
**作者：** Tech Researcher 🔬

