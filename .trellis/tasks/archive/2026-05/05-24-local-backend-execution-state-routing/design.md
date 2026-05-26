# 设计：本机后端执行状态与分配治理

## 设计原则

- `runtime_health` 表达连接健康，不表达执行占用。
- workspace inventory / binding 表达目录事实，不承担执行调度。
- backend execution lease 表达 session turn 对 backend 的占用，是自动分配与精确释放的事实源。
- `BackendSelectionRequest` 表达选择意图，`ExecutionPlacementPlan` 表达已解析的执行落点。
- session launch 先解析 owner/context/workspace，再做 execution placement，最后启动 connector。

## 架构边界

| 层 | 职责 |
| --- | --- |
| Domain | 定义 `BackendExecutionLease`、状态枚举、selection mode、repository trait |
| Infrastructure | PostgreSQL migration 与 repository 实现 |
| Application | `BackendExecutionAllocator`、lease claim/activate/release、session launch placement |
| API | HTTP DTO、鉴权、错误映射、runtime summary 查询 |
| Relay Registry | 在线连接、executor snapshot、session route map、pending command |
| Local Runtime | 继续执行 prompt/cancel/terminal notification，不直接写云端 DB |
| Frontend | 消费后端 summary，不自行推断 idle / busy |

## 数据模型

新增 `backend_execution_leases`：

```text
id TEXT PRIMARY KEY
backend_id TEXT NOT NULL REFERENCES backends(id) ON DELETE CASCADE
session_id TEXT NOT NULL
turn_id TEXT NOT NULL
executor_id TEXT NOT NULL
workspace_id TEXT
root_ref TEXT
selection_mode TEXT NOT NULL
state TEXT NOT NULL
claim_reason TEXT
terminal_kind TEXT
release_reason TEXT
claimed_at TEXT NOT NULL
activated_at TEXT
released_at TEXT
last_seen_at TEXT NOT NULL
created_at TEXT NOT NULL
updated_at TEXT NOT NULL
```

建议约束：

- `selection_mode IN ('explicit', 'auto_idle', 'workspace_binding')`
- `state IN ('claimed', 'running', 'released', 'lost', 'failed')`
- `UNIQUE(session_id, turn_id)`
- partial index：active states `claimed/running`
- index：`backend_id, state`

## Application 服务

新增 `BackendExecutionAllocator`：

输入：

- `executor_id`
- `selection_request`
- `workspace_binding` 或 workspace constraints
- allowed backend ids
- online executor snapshot
- active lease counts

输出：

- selected `backend_id`
- `lease_id`
- selection trace / warnings

策略：

1. 显式 backend：严格校验授权、在线、executor available、workspace binding 匹配。
2. auto idle：筛选满足 executor + workspace + 授权的在线 backend，按 active lease count 升序、backend_id 稳定排序。
3. workspace binding：workspace 解析只产出候选/默认绑定，最终仍进入 allocator。

第一版不引入 capacity / weight。这样 allocator 的权威输入保持为在线 executor snapshot、授权/backend access、workspace 约束与 active lease count，避免在状态事实源尚未稳定前扩大配置面。

## Session Launch 数据流

```text
API / internal launch command
  -> UserPromptInput.backend_selection
  -> SessionConstructionPlan resolves workspace/vfs/executor
  -> BackendExecutionAllocator resolves placement and claims lease
  -> LaunchPlan carries ExecutionPlacementPlan
  -> ExecutionContext.session.target_backend_id + lease_id
  -> RelayAgentConnector.prompt sends to selected backend
  -> prompt response activates lease
  -> terminal/cancel/disconnect releases or marks lease
```

`RelayAgentConnector` 不再自行用 VFS 猜 backend；它消费 LaunchPlan/ExecutionContext 中已选中的 backend。VFS mount 的 backend_id 仍用于文件系统路由与 workspace identity，也可以作为 launch placement 的 workspace-binding 候选来源，但最终落点由 allocator 校验。

## Relay Registry 扩展

`session_sinks` 从 `session_id -> sender` 扩展为 route entry：

```text
session_id -> {
  backend_id,
  lease_id,
  sender
}
```

用途：

- `cancel(session_id)` 精确定位 backend。
- backend disconnect 时找出归属该 backend 的 session routes，关闭 sink 并标记 lease lost。
- terminal 到达时由 session route 反查 lease 并 release。

## 查询投影

新增或扩展 backend runtime summary DTO：

- backend health：来自 `runtime_health` + registry online
- executor list：来自 registry executor snapshot
- active lease count：来自 lease repository
- active sessions：来自 active leases
- allocatable：后端在线、启用、executor 可用

前端使用该 DTO 展示空闲/忙碌和可分配状态。
当前实现提供 `GET /backends/runtime-summary`，并在 Settings 后端管理中展示 active session count 与 allocatable。

## 迁移与兼容

项目处于预研期，不保留旧行为兼容。实现时允许同步调整 API DTO、TS 类型与前端调用。数据库变更通过新增 PostgreSQL migration 完成，不修改既有 migration。

## 风险与处理

- prompt 已发送但 response 前失败：claim 后若 relay prompt 返回错误，lease 标记 failed。
- terminal notification 丢失：stalled/recovery 扫描将长时间 running 且无 route 的 lease 标记 lost。
- backend disconnect：registry unregister 释放 pending、关闭 routes，并标记 active lease lost。
- workspace binding 离线回退：执行 placement 不接受离线 backend 作为 auto/explicit 成功结果；如需保留目录解析 fallback，必须在 allocator 阶段失败。
