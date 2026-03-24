# sqlite-knowledge-graph 项目开发规划

## 项目概述

**目标：** 开发一个 SQLite 知识图谱 Rust 插件，用于优化 Aerial 的知识库和 Skill RAG 功能

**开发者：** Aerial (通过 OpenCode 多 Agent 协同)

## 测试数据

**数据源：** Aerial 的知识库 (`~/.openclaw/workspace/knowledge/knowledge.db`)
**备份要求：** 开发前必须备份现有数据

## Agent 分工

| Agent | 职责 |
|-------|------|
| **tech-researcher** | 技术调研：sqlite-vec、Rust SQLite 扩展 |
| **fullstack-engineer** | 核心开发：Rust 插件实现 |
| **test-agent** | 功能测试：单元测试、集成测试 |
| **tech-cofounder** | 项目管理、结果验收 |

## 开发阶段

### Phase 1: 技术调研 (Day 1)
- [ ] 调研 sqlite-vec API 和数据格式
- [ ] 调研 Rust SQLite 扩展开发框架 (rusqlite, sqlite-loadable)
- [ ] 设计数据库 Schema (entities, relations, vectors)
- [ ] 定义 Rust 插件 API 接口

**交付物：** 技术调研文档、Schema 设计、API 定义

### Phase 2: 核心开发 (Day 2-4)
- [ ] Rust 项目脚手架
- [ ] SQLite 扩展框架搭建
- [ ] 向量存储模块 (兼容 sqlite-vec)
- [ ] 知识图谱模块 (实体-关系-属性)
- [ ] 混合 RAG 检索模块

**交付物：** 可编译的 Rust 插件

### Phase 3: 测试验证 (Day 5)
- [ ] 单元测试
- [ ] 集成测试 (使用 Aerial 知识库备份数据)
- [ ] 性能基准测试
- [ ] RAG 检索效果评估

**交付物：** 测试报告、性能数据

### Phase 4: 验收部署 (Day 6)
- [ ] tech-cofounder 验收
- [ ] 文档完善
- [ ] 部署到 Aerial 知识库

**交付物：** 最终交付报告

## 数据库 Schema 设计

```sql
-- 实体表
CREATE TABLE entities (
    id INTEGER PRIMARY KEY,
    type TEXT NOT NULL,           -- 'paper', 'skill', 'concept'
    name TEXT NOT NULL,
    properties TEXT,              -- JSON
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 关系表
CREATE TABLE relations (
    id INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    type TEXT NOT NULL,           -- 'cites', 'relates_to', 'derived_from'
    weight REAL DEFAULT 1.0,
    properties TEXT,              -- JSON
    FOREIGN KEY (source_id) REFERENCES entities(id),
    FOREIGN KEY (target_id) REFERENCES entities(id)
);

-- 向量表 (兼容 sqlite-vec)
CREATE VIRTUAL TABLE vec_entities USING vec0(
    entity_id INTEGER PRIMARY KEY,
    embedding FLOAT[1536]         -- OpenAI embedding 维度
);

-- 全文搜索
CREATE VIRTUAL TABLE entity_search USING fts5(
    id, name, properties
);
```

## Rust 插件 API

```rust
// 知识图谱操作
fn kg_insert_entity(type: &str, name: &str, properties: &str) -> i64;
fn kg_insert_relation(source: i64, target: i64, rel_type: &str, weight: f64);
fn kg_get_entity(id: i64) -> Entity;
fn kg_find_neighbors(id: i64, depth: i32) -> Vec<Entity>;

// 向量操作
fn vec_insert(entity_id: i64, embedding: &[f32]);
fn vec_search(query: &[f32], k: i32) -> Vec<i64>;

// 混合 RAG
fn rag_search(query: &[f32], query_text: &str, k: i32) -> Vec<RagResult>;
```

## 备份策略

```bash
# 开发前备份
cp ~/.openclaw/workspace/knowledge/knowledge.db \
   ~/.openclaw/workspace/knowledge/knowledge.db.backup.$(date +%Y%m%d)
```

## 成功标准

1. [ ] 插件可加载到 SQLite
2. [ ] 向量搜索准确率 > 90%
3. [ ] 知识图谱查询正确
4. [ ] 混合 RAG 效果优于纯向量搜索
5. [ ] 性能: 1000 次查询 < 1 秒

---

**项目启动时间：** 2026-03-24
**预计完成时间：** 2026-03-30