# 存量 JSON 文本列 JSONB 收敛

## Goal

把仓库中仍以 `TEXT` 保存的结构化 JSON 业务文档收敛为 PostgreSQL `jsonb` / `json` / scalar / raw `TEXT` 的明确分类，并让 repository 通过 typed domain/value object 映射读写。默认目标是 `jsonb`。

目标不是为了追逐数据库类型洁癖，而是消除字符串协议带来的三个长期成本：

- 数据库无法验证结构化列一定是 JSON。
- repository 需要到处分散 `serde_json::from_str` / `to_string` 和手写错误上下文。
- 后续 owner-local document、capability surface、Channel registry、IM binding 等模型会继续复制旧范式。

本任务是数据库仓储卫生支线任务，不直接实现 ChannelService，也不是 Channel 的硬前置。Channel 新增列直接按当前数据库规范使用 `jsonb`；本任务并行清理存量 schema，避免旧范式继续扩散。

## Evidence

当前 schema 已经混用两套范式：

- `jsonb` 已在较新表/列中使用，例如 `agent_run_mailbox_messages.delivery_json/payload_json/executor_config_json`、permission grant JSONB、runtime health、extension manifest、shared library payload。
- 仍有大量结构化 JSON 以 `TEXT` 保存，例如 `lifecycle_runs.context/orchestrations/tasks/execution_log`、`agent_frames.*_json/surface`、`projects.config`、`project_agents.config`、`workflow_graphs.activities/transitions`、`stories.tags/context`、`views.backend_ids/filters`、`routines.trigger_config/dispatch_strategy/trigger_payload`、`llm_providers.models/blocked_models`、`workspaces.identity_payload/detected_facts`、`project_vfs_mounts.capabilities/content`。
- repository 中仍存在多处局部 `parse_json_column` / `serialize_json_column` helper；也已有 `sqlx::types::Json<T>` 的较新用法，可作为目标实现形态。

## Requirements

- R1: 盘点所有 live PostgreSQL schema 中“结构化 JSON in TEXT”的列，按以下结果分类：
  - convert to `jsonb`
  - convert to `json`
  - keep `TEXT` because it is raw text / user content / byte-preserving external payload
  - promote to scalar columns because it is frequently filtered, sorted, claimed, or constrained
  - defer because physical owner will be replaced by another near-term task
- R2: 每个候选列必须在 inventory 中明确目标类型。默认选择 `jsonb`；只有需要保留原始 JSON 文本形态、key 顺序或重复 key 语义时才选择 `json`；非 JSON document 的文本继续保持 `TEXT`。
- R3: 对明确属于结构化业务文档的列新增 migration，最终 schema type 通常为 `jsonb`，默认值使用 `'{}'::jsonb` 或 `'[]'::jsonb`；少数 `json` 列必须写明不能使用 `jsonb` 的业务原因。
- R4: repository 映射改为 `sqlx::types::Json<T>` 或窄共享 JSON document codec，业务层继续拿 typed domain/value object，不传播未建模的动态 JSON。
- R4a: 本任务不构建完整通用仓储脚手架；只允许抽出非常薄的 JSON typed bind/read/error helper，且不得改变 repository port、聚合边界或 application wiring。
- R5: 保留 raw text 的列必须在 inventory 中写明业务原因，例如 markdown/source body、用户输入文本、需要保留字节级原始内容的 provider payload。
- R6: 对现有 `_json` 后缀列做命名审计：如果后缀只是历史存储方式，应在本任务或后续明确整理任务中改为业务语义名；如果是 projection/debug/raw payload 语义，应写明原因。
- R7: migration 必须覆盖现有干净库初始化和从旧 schema 升级两条路径。
- R8: 更新 `.trellis/spec/backend/database-guidelines.md` 中与本次清理相关的示例或例外，确保后续新增结构化 document 不再回到 `TEXT`。

## Acceptance Criteria

- [ ] 新增 `research/text-json-column-inventory.md`，列出每个候选列、当前类型、repository owner、目标类型、分类结论和处理方式。
- [ ] 当前 live schema 中明确结构化的业务文档列已迁移为 `jsonb` / `json`，或在 inventory 中给出明确 defer 原因。
- [ ] repository 不再为已迁移列使用字符串 JSON roundtrip helper。
- [ ] `rg -n "parse_json_column|serialize_json_column|serde_json::from_str|serde_json::to_string" crates/agentdash-infrastructure/src/persistence/postgres` 的剩余命中均属于 scalar enum、raw text、未迁移 defer 列或测试。
- [ ] `rg -n "_json\\s+text|DEFAULT '\\{\\}'::text|DEFAULT '\\[\\]'::text" crates/agentdash-infrastructure/migrations` 对当前 schema target 不再出现未解释的结构化 document 列。
- [ ] `pnpm run migration:guard` 通过。
- [ ] 相关 PostgreSQL repository roundtrip 测试覆盖默认 document、非空 document、nullable document 和 shape mismatch 错误上下文。

## Out Of Scope

- 不借本任务重做聚合边界；Mailbox/Gate/Lineage 是否拆表或收回 owner document 另开数据库建模审计。
- 不实现 ChannelService。
- 不把本任务做成完整通用 repository scaffold；最多沉淀 JSON typed codec/helper。
- 不迁移非 PostgreSQL session runtime 存储。
- 不把真正的 raw text、用户正文、脚本源码、markdown、外部原始文本 payload 改成 `jsonb`。
