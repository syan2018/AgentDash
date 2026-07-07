# 存量 JSON 文本列 JSONB 收敛执行计划

## Current State

本任务处于 planning。已确认仓库当前同时存在：

- 新范式：`jsonb` columns + `sqlx::types::Json<T>` mapping。
- 旧范式：结构化 JSON 存在 `TEXT` 中，repository 手写 parse/serialize helper。

目标是把 live schema 和 repository 主线收敛到明确的 `jsonb` / `json` / scalar / raw `TEXT` 分类，不继续把结构化业务事实当字符串保存。默认业务文档目标是 `jsonb`。

## Phases

1. **Inventory**
   - 新增 `research/text-json-column-inventory.md`。
   - 从 migrations、repository row structs、mapper、insert/update SQL 中列出所有候选列。
   - 每列给出目标类型和分类：convert to jsonb / convert to json / keep text / promote scalar / defer。

2. **Migration Design**
   - 为 convert 列设计一个或多个 migration。
   - 对 nullable、not null、default object、default array 分别使用正确 `USING` 表达式。
   - 只有 inventory 明确要求保留 JSON 文本形态时才使用 `json`；其余结构化文档使用 `jsonb`。
   - 对需要重命名的 `_json` 列单独列出 rename 计划和 repository 影响。

3. **Repository Conversion**
   - 将已迁移列的 bind/read 改为 `sqlx::types::Json<T>` 或窄共享 JSON document codec。
   - 移除对应局部 `parse_json_column` / `serialize_json_column` helper。
   - 对仍保留 `TEXT` 的 raw text 列保留普通 string mapping。
   - 不引入完整 repository scaffold，不改变 repository trait 或 use-case deps。

4. **Tests**
   - 更新 PostgreSQL repository roundtrip tests。
   - 增加 migration upgrade fixture：旧 text JSON row 可迁为 jsonb。
   - 对 shape mismatch 保留带 `table.column` context 的错误。

5. **Spec Cleanup**
   - 如果执行中发现新的 JSONB codec / repository helper 约定，更新 `.trellis/spec/backend/database-guidelines.md`。
   - 若最终保留某些 `TEXT` JSON-looking 列，把业务原因回填到 inventory。

## Validation Commands

- `pnpm run migration:guard`
- 相关 PostgreSQL repository tests，范围由 inventory 最终列决定
- `rg -n "_json\\s+text|DEFAULT '\\{\\}'::text|DEFAULT '\\[\\]'::text" crates/agentdash-infrastructure/migrations`
- `rg -n "parse_json_column|serialize_json_column|serde_json::from_str|serde_json::to_string" crates/agentdash-infrastructure/src/persistence/postgres`

## Risk Notes

- `ALTER COLUMN ... TYPE jsonb USING col::jsonb` 会暴露历史坏数据；预研期应优先修正数据和 schema，而不是吞掉错误。
- `json` 是少数例外，不是折中默认；如果只是“可能以后要看原文”，应保留 raw `TEXT` 或另存 raw payload，而不是把业务 document 迁到 `json`。
- 有些 `TEXT` 字段只是名字像 JSON，但本质是 source text、markdown、用户文本或源码；inventory 必须先判定业务语义。
- `_json` 后缀清理会扩大 SQL、repository、测试影响面；如果无法一次完成，应先完成类型收敛并为命名整理留下明确 follow-up。
