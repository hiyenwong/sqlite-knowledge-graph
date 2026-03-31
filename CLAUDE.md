# Claude Code Instructions: sqlite-knowledge-graph

## Project Context

Read [AGENTS.md](AGENTS.md) first for full project overview.

This is a Rust library that compiles as a SQLite loadable extension.
`crate-type = ["cdylib", "rlib"]` — 同时构建动态库和静态库。

## Workflow

1. **Read** — 理解目标文件和相关域的现有实现
2. **Plan** — 确认修改范围，检查 docs/design/ 中的约定
3. **Implement** — 编写代码
4. **Test** — `cargo test`（本地必须全通过）
5. **Lint** — `cargo clippy -- -D warnings` + `cargo fmt`
6. **PR** — 提交，遵循 Conventional Commits 风格

## Hard Rules

- **禁止裸表名**：SQL 中必须使用 `kg_entities` / `kg_relations`，不得写 `entities` / `relations`
- **禁止 `.unwrap()`**：所有错误必须用 `?` 传播或显式处理
- **禁止跳过 CI 检查**：不得加 `--no-verify` 或跳过 clippy
- **禁止修改 crate-type**：`cdylib` + `rlib` 两者都需要

## Environment

- Rust 最低版本：1.70
- 依赖管理：`cargo`（不用 uv/pip）
- 无 Python/Node 工具链依赖

## Known Critical Bugs (P0)

实现前先查 [docs/quality/domains.md](docs/quality/domains.md) 确认当前 P0 状态。
修复 P0 优先于新功能。
