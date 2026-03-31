# Agent Guide: sqlite-knowledge-graph

## Quick Start

```bash
cargo build                          # 编译库 + SQLite 扩展
cargo test                           # 运行所有测试
cargo fmt                            # 格式化代码
cargo clippy -- -D warnings          # Lint（CI 强制通过）
cargo doc --no-deps                  # 生成文档
```

## Architecture

层级依赖方向（单向，不得逆向引用）：

```
schema (kg_entities / kg_relations 表定义)
  ↓
graph (entity · relation · hyperedge · traversal)
  ↓                      ↓
algorithms            vector
(pagerank/louvain/    (turboquant/store)
 connected)
  ↓                      ↓
          rag
    (检索增强生成框架)
          ↓
         export
```

详见 [docs/architecture/README.md](docs/architecture/README.md)

## Key Domains

| 域 | 路径 | 设计文档 | 状态 |
|----|------|----------|------|
| schema | `src/schema.rs` + `src/migrate.rs` | — | 稳定 |
| graph | `src/graph/` | [docs/design/graph.md](docs/design/graph.md) | 稳定 |
| algorithms | `src/algorithms/` | [docs/design/algorithms.md](docs/design/algorithms.md) | P0 Bug |
| vector | `src/vector/` | [docs/design/vector.md](docs/design/vector.md) | P0 Bug |
| rag | `src/rag/` | [docs/design/rag.md](docs/design/rag.md) | 空壳 |
| export | `src/export/` | — | 稳定 |

## Critical Constraints

### SQL 表名规范（P0）
- **正确**：`kg_entities`、`kg_relations`
- **错误**：`entities`、`relations`（裸名会在真实数据库 100% 崩溃）
- 集成测试必须调用 `create_schema()` 初始化，不得使用临时 DDL

### Hyperedge 错误处理
- 禁止裸 `.unwrap()`，使用 `?` 传播或 `.map_err()`
- 已知 panic 点：`hyperedge.rs:476, 532, 558`

### TurboQuant（向量量化）
- 旋转矩阵必须通过 QR 分解生成正交矩阵，不得用随机均匀矩阵
- Codebook 使用 Max-Lloyd 算法，不得用估计分位数
- 相似度归一化需在量化空间内完成

## Conventions

- **错误类型**：各域有独立 `error.rs`，使用 `thiserror`，向上用 `?` 传播
- **日志**：使用 `tracing` crate，结构化字段（`tracing::debug!(entity_id = %id, ...)`）
- **函数签名**：所有公开函数必须有类型标注
- **文件大小**：单文件不超过 300 行，超出时拆分为子模块
- **SQL**：只在 `schema.rs` / `store.rs` 中写 SQL，业务层调用函数

## Quality Gates

| 检查项 | 命令 | 要求 |
|--------|------|------|
| 编译 | `cargo build` | 零错误 |
| 测试 | `cargo test` | 全通过 |
| 格式 | `cargo fmt -- --check` | 零 diff |
| Lint | `cargo clippy -- -D warnings` | 零警告 |

## Agent Workflow

1. 读本文件了解项目全貌
2. 查 [docs/design/](docs/design/) 获取目标域的详细设计
3. 查 [docs/quality/domains.md](docs/quality/domains.md) 了解已知问题
4. 实现 → `cargo test` → `cargo clippy` → `cargo fmt`
5. 提 PR，PR 模板见 `.github/PULL_REQUEST_TEMPLATE.md`

## Where to Learn More

| 资源 | 路径 |
|------|------|
| 架构图 | [docs/architecture/README.md](docs/architecture/README.md) |
| 域设计文档 | [docs/design/README.md](docs/design/README.md) |
| 质量状态 | [docs/quality/domains.md](docs/quality/domains.md) |
| 技术债务 | [docs/debt.md](docs/debt.md) |
| 执行计划 | [docs/plans/active/](docs/plans/active/) |
| 变更日志 | [CHANGELOG.md](CHANGELOG.md) |
| 研究背景 | [research.md](research.md) |
