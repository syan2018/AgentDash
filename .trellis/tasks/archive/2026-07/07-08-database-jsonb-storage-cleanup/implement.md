# 存量 JSON 文本列 JSONB 收敛执行追踪

## 当前进度

状态：Phase 3 提交与 PR。

已完成：

- 三路并行盘点：schema/migration、runtime/workflow repository、product/config repository。
- 汇总 inventory：`research/text-json-column-inventory.md`。
- 新增 migration：`0058_json_text_columns_to_jsonb.sql`。
- PostgreSQL repository 读写从 JSON 字符串协议收束到 `jsonb` typed mapping / `serde_json::Value` 边界 helper。
- Scalar enum 字段保持 PostgreSQL `text`，迁移中只规范化历史 JSON-string 值。
- 原生 `trellis-check` 子代理 `Bacon` 已完成全量 diff 复核，无未修复问题。

正在进行：

- 主线程提交、归档、记录 journal、创建 PR。

## 派发链路

本任务采用主线程集成 + 并行 worker 盘点/检查：

| Worker | 范围 | 输出 | 状态 |
| --- | --- | --- | --- |
| Herschel | migration/source schema 中 TEXT JSON live inventory | `research/schema-text-json-inventory.md` | done |
| Singer | workflow/runtime/session/mailbox/state-change repository inventory | `research/runtime-workflow-repository-inventory.md` | done |
| Hubble | product/config/workspace/canvas/routine repository inventory | `research/product-config-repository-inventory.md` | done |
| Bacon | 全量 diff check、验证、可确定问题直接修复 | final answer / code edits if needed | done |

主线程负责：

- 合并 research 结论并解决冲突。
- 维护 migration 与 repository 代码一致性。
- 跑最终验证命令。
- 统一提交、归档、journal、PR。

## Work Items

每个工作项有独立追踪文件：

- `workitems/WI-01-inventory.md`
- `workitems/WI-02-migration.md`
- `workitems/WI-03-product-config-repositories.md`
- `workitems/WI-04-workflow-runtime-repositories.md`
- `workitems/WI-05-session-auth-state-mailbox.md`
- `workitems/WI-06-validation-pr.md`

当前状态：

| Work Item | 状态 | 说明 |
| --- | --- | --- |
| WI-01 Inventory | done | 三份 worker 盘点已汇总成最终 inventory。 |
| WI-02 Migration | done | 0058 forward migration 覆盖 live structured TEXT JSON 列；scalar enum 归 text。 |
| WI-03 Product/config repositories | done | project/agent/backend/story/workspace/canvas/settings/LLM/MCP/routine/VFS/backend-access 已转换。 |
| WI-04 Workflow/runtime repositories | done | workflow/lifecycle/gate/lineage/runtime session 路径已转换。 |
| WI-05 Session/auth/state/mailbox | done | session_core、auth session、state change、mailbox source metadata 已转换。 |
| WI-06 Validation/PR | done | 本地验证和独立 check 均已通过，进入提交/PR。 |

## 验证命令

已通过：

```bash
cargo fmt
cargo fmt --check
cargo check -p agentdash-infrastructure
cargo check --workspace
TEST_DATABASE_URL='' DATABASE_URL='' cargo test -p agentdash-infrastructure --lib
pnpm run migration:guard
rg -n 'parse_json_column|serialize_json_column|serde_json::from_str|serde_json::to_string|json_string|trim_matches' crates/agentdash-infrastructure/src/persistence/postgres crates/agentdash-infrastructure/src/persistence/session_core.rs -S
```

Migration history grep 会命中历史 migration；最终判断看 `0058_json_text_columns_to_jsonb.sql` 的 target conversion 和 `research/text-json-column-inventory.md` 的分类结论。

## 压缩恢复方式

压缩后恢复主持上下文时按顺序读取：

1. `AGENTS.md` 和 `.trellis/workflow.md`。
2. `.trellis/tasks/07-08-database-jsonb-storage-cleanup/prd.md`、`design.md`、本文件。
3. `research/text-json-column-inventory.md`。
4. `workitems/*.md`。
5. `git status --short --branch` 与当前 diff。

然后继续提交、归档、记录 journal、创建 PR。
