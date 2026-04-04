# Quality Status by Domain

Last updated: 2026-04-02 | Version: v0.10.3

## Summary

| 域 | P0 | P1 | P2 | 整体状态 |
|----|----|----|----|---------|
| graph | 0 | 0 | 1 | 🟡 P2 待修 |
| algorithms | 0 | 0 | 0 | 🟢 稳定 |
| vector | 0 | 0 | 0 | 🟢 稳定 |
| rag | 0 | 0 | 0 | 🟢 稳定 |
| schema | 0 | 0 | 0 | 🟢 稳定 |
| export | 0 | 0 | 0 | 🟢 稳定 |

## P0 — 全部已修复 ✅

| ID | 描述 | 修复时间 |
|----|------|---------|
| P0-1 | SQL 裸表名 → `kg_entities`/`kg_relations` | c168a9c |
| P0-2 | TurboQuant 旋转矩阵 → QR 分解正交矩阵 | c168a9c |
| P0-3 | TurboQuant Codebook → Max-Lloyd 1D k-means | c168a9c |
| P0-4 | TurboQuant 相似度归一化 → 量化空间内计算 | c168a9c |
| P0-5 | `hyperedge.rs:270` + `entity.rs:162` 裸 `.unwrap()` → `map_err()?` | 2026-03-31 |

## P1 — 全部已修复 ✅

| ID | 描述 | 修复时间 |
|----|------|---------|
| P1-1 | Louvain Phase 2（超级节点聚合）实现 | 2026-03-31 |
| P1-2 | Louvain modularity gain 公式 | c168a9c |
| P1-3 | BFS 深度计算 → 队列存 `(node, depth)` | c168a9c |
| P1-4 | `update_entity` 静默成功 → `EntityNotFound` | c168a9c |
| P1-5 | `get_vector` 双重 SQL → 单次 SELECT | c168a9c |

## P2 — 全部已修复 ✅

| ID | 域 | 描述 | 修复时间 |
|----|----|----|---------|
| P2-1 | graph | `level_queue` 死代码 | c168a9c |
| P2-2 | vector | 批量插入逐条验证 entity | c168a9c |
| P2-3 | graph | SCC 迭代实现，风险已降低 | 已关闭 |
| P2-4 | rag | `search()` 纯空壳 → 完整两阶段 RAG 引擎 | 95f98ae |

## 下一步

所有 P0/P1/P2 问题已全部修复。下一步可考虑：
- RAG `SubprocessEmbedder` 集成测试（需外部 Python 服务）

## 已完成优化

- **持久化 TurboQuant 索引** ✅（v0.10.2）：索引序列化存入 `kg_turboquant_cache` 表，以向量数量为版本号，同一 DB 多次 RAG 查询只建一次索引
- **Schema 自动迁移** ✅（v0.10.3）：`kg_schema_version` 表 + `ensure_schema()` 迁移运行器，支持增量升级（含旧 DB 探测）；`create_schema()` 保持向后兼容
- **缓存失效策略升级** ✅（v0.10.3）：`kg_turboquant_cache` 新增 `vectors_checksum`（`SUM(entity_id)`），count + checksum 双重校验，防止同等数量但不同向量集导致的缓存误判
