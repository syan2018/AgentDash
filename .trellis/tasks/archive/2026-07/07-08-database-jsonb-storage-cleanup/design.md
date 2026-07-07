# 存量 JSON 文本列 JSONB 收敛设计

## 第一性边界

PostgreSQL 里有三类完全不同的事实，不应都用 `TEXT` 承载：

| 类型 | 目标存储 | 原因 |
| --- | --- | --- |
| 结构化业务文档 | `jsonb` + typed repository mapping | 数据库验证 JSON 形态，repository 不再维护字符串协议 |
| 需要保留 JSON 文本形态的文档 | `json` + typed repository mapping | 保留 key 顺序、重复 key 或原始 JSON 文本语义 |
| 高频查询/排序/约束字段 | scalar columns | 查询计划、索引和约束应显式表达业务事实 |
| 原始文本/源码/markdown/字节级 provider payload | `TEXT` | 业务要求保留文本本身，而不是 JSON document |

因此本任务不按列名机械转换，而按 repository 读写语义转换。凡是 repository 以 `serde_json` 解析成 domain struct、enum、list、map、capability surface、runtime document 的列，默认进入 `jsonb` 候选。

## JSON vs JSONB 选择规则

默认选择 `jsonb`。它更适合业务文档、owner-local document、capability surface、runtime registry、配置对象、结构化 payload 和后续可能需要 operator / expression index 的列。

选择 `json` 只允许出现在这些情况：

- 业务语义要求保留 JSON object key 顺序。
- 业务语义要求保留重复 key 的原始 JSON 表达。
- 列承担的是“已验证为 JSON 的原始外部文档”，而不是系统自己的 normalized business document。

继续使用 `TEXT` 的情况：

- 字段不是 JSON document，而是 markdown、脚本源码、用户正文、纯文本模板、日志文本或 provider 原始 body。
- 字段必须字节级保留原始输入，连 JSON parser normalization 都不能接受。

提升为 scalar columns 的情况：

- 字段参与高频过滤、排序、唯一约束、claim/lease、状态机推进或权限判断。
- 需要数据库约束表达跨字段 invariant。

## 初始候选范围

初始 grep 发现的高置信候选包括：

- `lifecycle_runs.context/orchestrations/tasks/execution_log`
- `agent_frames.surface` 与 `effective_capability_json/context_slice_json/vfs_surface_json/mcp_surface_json/execution_profile_json/visible_*_json`
- `projects.config`、`project_agents.config`
- `workflow_graphs.activities/transitions`
- `stories.tags/context`
- `views.backend_ids/filters`
- `llm_providers.models/blocked_models`
- `routines.trigger_config/dispatch_strategy/trigger_payload`
- `workspaces.identity_payload/detected_facts`、workspace binding facts
- `project_vfs_mounts.capabilities/content`
- `state change` / session persistence 中的 structured payload columns，如 `replacement_projection_json/token_stats_json/diagnostics_json/source_refs_json/content_json/payload_json`
- `agent_run_mailbox_messages.source_metadata`
- `agent_run_lineages.metadata`
- canvas runtime state `payload`，如果 repository 始终把它当 typed observation/snapshot

需要人工分类的候选包括：

- `agent_procedures.source/contract`、`workflow_graphs.source`：如果是 structured procedure/workflow document，应迁为 `jsonb`；如果保留源码文本或外部原文，则保持 `TEXT`。
- `settings.value`：如果 settings value 是任意 JSON document，应迁为 `jsonb`；如果它是当前设置系统的字符串值协议，需先调整 settings domain。
- `canvas_files.content`、inline file text、script source：通常保持 `TEXT`。

## Migration Pattern

非空 object/list document：

```sql
ALTER TABLE lifecycle_runs
    ALTER COLUMN orchestrations DROP DEFAULT,
    ALTER COLUMN orchestrations TYPE jsonb USING orchestrations::jsonb,
    ALTER COLUMN orchestrations SET DEFAULT '[]'::jsonb,
    ALTER COLUMN orchestrations SET NOT NULL;
```

nullable document：

```sql
ALTER TABLE agent_frames
    ALTER COLUMN surface TYPE jsonb
    USING CASE
        WHEN surface IS NULL OR NULLIF(BTRIM(surface), '') IS NULL THEN NULL
        ELSE surface::jsonb
    END;
```

少数 `json` document 使用相同结构，但目标类型和默认值使用 `json`：

```sql
ALTER TABLE external_documents
    ALTER COLUMN raw_document TYPE json USING raw_document::json;
```

需要重命名的列先在 design inventory 中确认目标业务名，再通过同一 migration 完成 rename + type conversion。重命名只在业务名清楚时执行；否则本任务先完成类型收敛，后续命名整理单独派发。

## Repository Pattern

目标写法优先使用 `sqlx::types::Json<T>`：

```rust
use sqlx::types::Json;

query.bind(Json(&run.orchestrations));

let orchestrations: Json<Vec<OrchestrationInstance>> = row.try_get("orchestrations")?;
let orchestrations = orchestrations.0;
```

对于 provider 原始 payload 或 schema 未知的 extension payload，可使用 `Json<serde_json::Value>`，但该 `Value` 不应越过 repository/application ingress 边界成为核心 domain 事实。

本任务不引入完整通用 repository scaffold。允许抽出的公共能力只限于：

- `Json<T>` bind/read 的薄封装。
- 带 `table.column` 的统一反序列化错误映射。
- nullable JSON document 的 row mapping helper。

这些 helper 不拥有 repository trait、不改变 transaction boundary、不参与 application wiring。

## Baseline Strategy

默认实施方式是新增 forward migration，把当前 live schema 收敛到 `jsonb` target，并同步 repository。

如果本任务被明确授权做数据库 baseline squash / reset，则可以把目标 schema 折回 `0001_init.sql` 并重建开发数据库。该路径必须遵守 `.trellis/spec/backend/database-guidelines.md` 的 baseline squash 规则，并在任务记录中写明重建要求。

## Validation

- migration guard 覆盖干净库初始化。
- 升级测试覆盖旧 `TEXT` row 到 `jsonb` 的转换。
- repository roundtrip 覆盖 typed document。
- grep 剩余 `TEXT` JSON 和 string helper 命中，逐项对应 inventory 结论。
