# sqlite-knowledge-graph Phase 3: RAG Integration Plan

## 目标
将 Aerial 的知识库集成到 sqlite-knowledge-graph，实现高效 RAG 检索。

## 当前知识库状态

| 指标 | 数值 |
|------|------|
| 论文总数 | 2,497 |
| 已创建技能 | 303 |
| 技能文件 | 122 |
| 高效用论文 | 455 |

## Phase 3 开发任务

### 1. 数据迁移模块
- [ ] 从 knowledge.db 迁移论文到 kg_entities
- [ ] 从 skills/ 目录创建技能实体
- [ ] 建立论文-技能关系

### 2. 关系发现
- [ ] 基于关键词建立论文关系
- [ ] 基于技能相似度建立关系
- [ ] 引用关系（如果有数据）

### 3. 向量嵌入
- [ ] 为论文标题生成嵌入
- [ ] 为技能内容生成嵌入
- [ ] 实现语义相似度搜索

### 4. RAG 查询接口
- [ ] kg_rag_search(query_text, k) - 混合搜索
- [ ] kg_get_context(entity_id, depth) - 获取上下文
- [ ] kg_find_related(entity_id, threshold) - 找相关实体

### 5. 性能优化
- [ ] 向量索引（IVF 或 HNSW）
- [ ] 查询缓存
- [ ] 批量操作优化

## Schema 设计

```sql
-- 论文实体
INSERT INTO kg_entities (type, name, properties)
VALUES ('paper', '论文标题', '{"arxiv_id": "...", "utility": 0.95}');

-- 技能实体
INSERT INTO kg_entities (type, name, properties)
VALUES ('skill', '技能名称', '{"source_paper": "arxiv_id"}');

-- 论文-技能关系
INSERT INTO kg_relations (source_id, target_id, rel_type, weight)
VALUES (paper_id, skill_id, 'derived_from', 1.0);

-- 论文-论文关系
INSERT INTO kg_relations (source_id, target_id, rel_type, weight)
VALUES (paper1_id, paper2_id, 'related_to', similarity_score);
```

## 预期效果

| 场景 | 当前 | 改进后 |
|------|------|--------|
| 技能检索 | 关键词匹配 | 向量+图谱混合 |
| 相关论文 | 手动查找 | 自动推荐 |
| 知识关联 | 无 | 图谱可视化 |
| RAG 效率 | 低 | 高 |

---

**创建时间：** 2026-03-25 00:35
**状态：** 规划中