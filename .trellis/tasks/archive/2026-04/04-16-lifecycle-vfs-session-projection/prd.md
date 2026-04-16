# Lifecycle VFS 收尾 + Session 投影

## Goal

完成 lifecycle_vfs 的 inline_fs 收尾工作，并新增 session 记录的混合投影能力（摘要物化 + turns 虚拟投影），使后继 node agent 可按需读取前驱的 session 上下文。

## Background

### inline_fs 迁移现状

lifecycle VFS 的 `artifacts/{port_key}` 读写已全部通过 `InlineFileRepository` 完成（`provider_lifecycle.rs`）。但存在残留：
- `LifecycleRun.port_outputs: BTreeMap<String, String>` 实体字段仍在，通过 dual-write + hydrate 模式维持兼容
- `workflow_repository.rs` 中 `sync_port_outputs_to_inline_fs()` / `hydrate_port_outputs()` 双写桥梁仍在
- `mount.rs` 中 `inline_files_from_mount()` 已是死代码
- `lifecycle_runs` 表的 `port_outputs` 列仍在被写入

### record_artifacts 已清理

`04-16-cleanup-record-artifacts` task 已移除整套 record_artifacts 体系，lifecycle VFS 中所有 `active/artifacts/*` 和 `nodes/*/artifacts/*` 路径已删除。

### Session 投影需求

当前 lifecycle 后继 node agent 获取前驱上下文的唯一方式是 port_outputs（显式产出）。大量有价值信息（推理过程、尝试过的方案、遇到的错误）未被捕获。需要在 lifecycle VFS 中暴露 session 记录，供后继 agent 按需主动读取（不自动注入 context）。

## Requirements

### Part A: inline_fs 收尾

#### A1: 移除 `LifecycleRun.port_outputs` 实体字段

- 删除 `entity.rs` 中 `pub port_outputs: BTreeMap<String, String>` 字段
- 删除 `LifecycleRun::new()` 中 `port_outputs: BTreeMap::new()` 初始化
- 所有对 `run.port_outputs` 的直接读取改为通过 `InlineFileRepository` 查询

**影响面盘点**：

| 用途 | 位置 | 当前读取源 | 改造方式 |
|------|------|-----------|---------|
| VFS read/write/list | `provider_lifecycle.rs` | 已走 `InlineFileRepository` | 无需改动 |
| 门禁 port 值 | `advance_node.rs` | `run.port_outputs` | 改为 `InlineFileRepository.get_file()` |
| hook snapshot | `hooks/provider.rs` | 间接 | 核查是否直接读 entity |
| orchestrator 上下文 | `orchestrator.rs` | 间接通过 VFS | 无需改动 |

#### A2: 移除 dual-write 桥梁

- 删除 `workflow_repository.rs` 中 `sync_port_outputs_to_inline_fs()` 函数
- 删除 `hydrate_port_outputs()` 函数
- INSERT/UPDATE 中 `port_outputs` 列写死 `'{}'`
- SELECT 中不再读取 `port_outputs` 列（或读取后忽略）

#### A3: 清理死代码

- 删除 `mount.rs` 中 `inline_files_from_mount()` 函数（如确认无调用方）
- 清理其他因 record_artifacts 删除和 port_outputs 移除产生的未使用 import/函数

### Part B: Session 投影

#### B1: LifecycleMountProvider 新增 `SessionPersistence` 依赖

```rust
pub struct LifecycleMountProvider {
    lifecycle_run_repo: Arc<dyn LifecycleRunRepository>,
    inline_file_repo: Arc<dyn InlineFileRepository>,
    session_persistence: Arc<dyn SessionPersistence>,  // 新增
}
```

#### B2: VFS 路径 — 虚拟投影（只读）

从 `LifecycleStepState.session_id` 关联 session，实时投影以下路径：

| 路径 | 数据源 | 说明 |
|------|--------|------|
| `nodes/{step_key}/session/meta` | `SessionPersistence` | session 元信息（status, title, turn count, created_at） |
| `nodes/{step_key}/session/turns` | `session_events` 表 | turn 列表摘要（turn_id, timestamp, message preview） |
| `nodes/{step_key}/session/turns/{turn_id}` | `session_events` 表 | 单个 turn 的完整消息流（user + assistant + tool calls） |

虚拟投影特性：
- 只读，不可写
- session 被清理后返回 NotFound
- agent 可 list `nodes/{step_key}/session/` 获知有哪些子路径可用

#### B3: VFS 路径 — 物化摘要（inline_fs 持久化）

物化内容存入 `inline_fs_files`，`owner_kind = LifecycleRun`，`container_id = "session_records"`：

| 路径 | 物化时机 | 说明 |
|------|---------|------|
| `nodes/{step_key}/session/summary` | step 完成时 | session 摘要（从 step_state.summary 或 session terminal message 提取） |
| `nodes/{step_key}/session/conclusions` | step 完成时 | 最终结论/决策记录 |

物化触发点：
- `LifecycleRunService::complete_step()` 中，完成 step 时自动物化 summary
- 物化内容来源优先级：`step_state.summary` > `session.last_terminal_message` > 空

#### B4: 读取优先级

对于 `nodes/{step_key}/session/summary` 路径：
1. 先查 `inline_fs_files`（物化副本）
2. 如果不存在，尝试从 session 实时构建（fallback）

对于 `nodes/{step_key}/session/turns` 路径：
- 仅虚拟投影，不物化

#### B5: list 支持

`nodes/{step_key}/` 的 list 结果中包含 `session/` 子目录（仅当 `step_state.session_id.is_some()` 时）。

`nodes/{step_key}/session/` 的 list 结果包含 `meta`、`summary`、`conclusions`、`turns/`。

## Acceptance Criteria

**Part A:**
- [ ] `LifecycleRun` entity 不再持有 `port_outputs` 字段
- [ ] `advance_node.rs` 门禁检查改用 `InlineFileRepository`
- [ ] dual-write 桥梁代码已删除
- [ ] `inline_files_from_mount()` 死代码已清理
- [ ] `cargo build` 通过
- [ ] 现有 port_outputs VFS 读写不受影响

**Part B:**
- [ ] `LifecycleMountProvider` 构造函数接受 `SessionPersistence`
- [ ] `nodes/{step_key}/session/meta` 返回 session 元信息
- [ ] `nodes/{step_key}/session/turns` 返回 turn 列表
- [ ] `nodes/{step_key}/session/turns/{turn_id}` 返回单 turn 消息流
- [ ] `nodes/{step_key}/session/summary` 先查 inline_fs 物化，后 fallback 实时
- [ ] step 完成时自动物化 summary 到 inline_fs
- [ ] list `nodes/{step_key}/session/` 正确列出子路径
- [ ] 无 session 的 step 不暴露 session/ 路径
- [ ] `cargo build` 通过
- [ ] `npm run build` 通过

## Implementation Phases

### Phase 1: Part A — inline_fs 收尾

1. 盘点 `run.port_outputs` 的所有直接读取点，逐一改为 `InlineFileRepository`
2. 删除 `LifecycleRun.port_outputs` 字段
3. 删除 dual-write/hydrate 函数
4. 清理死代码
5. 编译通过

### Phase 2: Part B — Session 投影基础

1. `LifecycleMountProvider` 新增 `SessionPersistence` 依赖
2. 实现 `nodes/{step_key}/session/meta` 虚拟投影
3. 实现 `nodes/{step_key}/session/turns` 和 `turns/{turn_id}` 虚拟投影
4. 更新 list 支持
5. 编译通过

### Phase 3: Part B — 物化摘要

1. `complete_step()` 中新增 summary 物化到 inline_fs
2. 实现 `nodes/{step_key}/session/summary` 和 `conclusions` 的混合读取
3. 编译通过 + 端到端功能验证

## Technical Notes

- `SessionPersistence` trait 定义在 `crates/agentdash-application/src/session/persistence.rs`，方法 `read_backlog()` 可获取 session 事件流
- session_events 按 `(session_id, event_seq)` 索引，支持高效顺序读取
- turn 按 `turn_id` 分组，每个 turn 包含 user_message → assistant_message → tool_calls → tool_results
- `LifecycleStepState.session_id` 是 step → session 的关联字段，AgentNode 类型的 step 在启动子 session 时设置
- `advance_node.rs` 的门禁检查当前直接读 `run.port_outputs`，Part A 需要注入 `InlineFileRepository` 到 tool 或通过其他方式获取（可能需要在 tool context 中传递 repo 引用）

## Related Tasks

- `04-16-cleanup-record-artifacts` — 前置已完成，移除了 record_artifacts 体系
- `04-16-inline-fs-storage-refactor` — 父级 task，本 task 完成其 lifecycle 侧剩余工作
- `04-16-step-level-ports` — port 归属迁移，与 Part A 的 port_outputs 清理有交集
