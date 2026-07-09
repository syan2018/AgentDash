# 技术设计

## 架构边界

backend selection 继续归属 `LaunchPlanningInput`。HTTP/API、AgentRun mailbox、scheduler 和 session launch 只负责携带 planning facts；最终 execution placement 仍由 session launch planner 解析并 claim backend execution lease。

核心原则：

- 用户输入 DTO 表达本轮 placement intent。
- mailbox 持久化本轮 planning facts，保证 queued/retry 后语义不丢。
- AgentRun/LifecycleAgent 持久化 sticky default backend preference，作为后续无额外指令时的 planning input 来源。
- planner 在同一层完成授权范围、workspace/root 一致性、executor 可用性和 lease claim。
- 前端只提交用户选择，不自行推断最终可执行 backend；最终失败必须从 API / mailbox / projection 回到用户可见状态。

## 数据模型

### Wire DTO

在 contract crate 中新增或复用 generated backend selection DTO：

```rust
pub struct BackendSelectionRequestDto {
    pub mode: BackendSelectionModeDto,
    pub backend_id: Option<String>,
}

pub enum BackendSelectionModeDto {
    Explicit,
    AutoIdle,
    WorkspaceBinding,
}
```

DTO 消费点：

- `CreateProjectAgentRunRequest.backend_selection`
- `AgentRunComposerSubmitRequest.backend_selection`

字段使用 `snake_case` wire shape，并生成到 `project-agent-contracts.ts` 与 `agent-run-mailbox-contracts.ts`。

### Mailbox

`AgentRunMailboxMessage` / `NewAgentRunMailboxMessage` 增加 `launch_planning_json` 或等价 typed persistence 字段。内容至少包含：

- `backend_selection`
- selection source: request explicit / sticky default / workspace binding / auto idle
- optional resolved/default backend preference snapshot

该字段进入 command receipt digest。message cleanup 不应删除 planning facts；payload cleanup 只清用户输入正文。

### Sticky Preference

为 AgentRun 当前默认 backend preference 增加持久化事实。优先落在 lifecycle agent / AgentRun runtime state 侧，而不是 runtime session meta，原因是 preference 属于 AgentRun control-plane 行为，不属于单条 trace。

建议字段：

```text
lifecycle_agents.default_backend_selection_json nullable
```

内容示例：

```json
{
  "mode": "explicit",
  "backend_id": "backend-a",
  "updated_by_mailbox_message_id": "...",
  "updated_by_turn_id": "...",
  "updated_at": "..."
}
```

只有 explicit selection 成功 accepted 后更新。失败、queued 未消费、steer 不更新。

## Launch Flow

### Draft Start

```text
CreateProjectAgentRunRequest
  -> ProjectAgentRunStartCommand
  -> initial mailbox message with launch_planning_json
  -> scheduler consumes message
  -> AgentRunMessageDelivery includes LaunchPlanningInput
  -> session launch planner resolves placement
  -> connector accepted
  -> update sticky explicit preference if applicable
```

### Composer Submit

```text
AgentRunComposerSubmitRequest
  -> AgentRunMailboxUserMessageCommand
  -> mailbox message stores input + executor_config + launch_planning_json
  -> immediate or later scheduler consume
  -> delivery passes LaunchPlanningInput
```

### No Selection

当 request 没有 selection：

1. 读取 AgentRun sticky explicit preference。
2. 校验该 backend 仍在 Project active grants 且能承载 current workspace/root。
3. 若无 sticky preference，使用 current workspace binding。
4. 若无 workspace binding，使用授权范围内 auto idle。

sticky preference 失效时不静默回退。返回用户可见错误，要求重新选择或修复授权/绑定。

## Authorization And Placement

planner 输入需要包含 Project/AgentRun control-plane context 或一个 application-level placement policy port，用于读取：

- current Project id
- active ProjectBackendAccess grants
- current workspace binding / VFS mount metadata
- backend executor availability

解析规则：

- explicit: backend_id 必填，必须在 active grants，必须有可用 executor，必须匹配或可解析 current workspace root。
- workspace_binding: backend_id 必填或由 current VFS anchor 提供，必须在 active grants。
- auto_idle: candidates = active grants intersect online available executor backends。

root/backend 一致性规则：

- 如果 selected backend 等于 VFS default mount backend，可直接使用 default mount root。
- 如果不同，必须尝试从 current workspace 的 bindings 选择 selected backend 的 ready binding，并重建 launch VFS/root。
- 如果无法重建，拒绝 launch，错误必须指明 selected backend 缺少当前 workspace binding。

## Error Propagation

错误应在三处可见：

- API immediate failure: route 返回明确 `ApiError` message 和 code，`SessionChatView` inline sendError 展示短摘要，同时外层通知展示完整错误内容。
- mailbox consume failure: message status `failed`，`last_error` 写入 placement/launch 失败原因，workspace mailbox row 显著展示短摘要，同时外层通知或详情面板展示完整错误内容。
- runtime projection: AgentRun workspace shell / conversation status 包含最近 backend placement failure summary，避免用户必须展开 mailbox 才能发现。

inline composer banner 和 mailbox row 不是完整错误信道。它们可以为了布局做摘要，但必须提供不截断的错误面：

- 立即失败：弹出全局 toast/notification，内容包含完整 message、错误码、backend id、workspace/root 相关上下文和可操作建议。
- 异步失败：从 mailbox/projection 观察到新的 failed/blocked placement error 时弹出通知；同时在 mailbox row 提供“查看错误”入口。
- 重复投递同一个失败事件时要按 command receipt / mailbox message / error code 去重，避免 toast 风暴。
- 完整错误内容不得只放在 HTML title 或被 `truncate` 的文本节点里。

建议错误码：

- `backend_selection_unauthorized`
- `backend_selection_offline`
- `backend_executor_unavailable`
- `backend_workspace_binding_missing`
- `backend_workspace_root_mismatch`
- `backend_selection_stale_default`

## Migration

需要 PostgreSQL migration：

- `agent_run_mailbox_messages.launch_planning_json text/jsonb null`
- `lifecycle_agents.default_backend_selection_json text/jsonb null`

不需要兼容旧数据。现有 null 表示没有 sticky preference / 没有 per-message selection。

## Tests

后端：

- contract generation 包含 request 字段。
- mailbox digest 区分不同 backend selection。
- queued message 消费保留 selection。
- explicit accepted 后更新 sticky preference。
- failed explicit 不更新 sticky preference。
- auto idle 只在 active grants 中选。
- explicit 未授权、离线、无 executor、workspace binding 缺失分别失败。

前端：

- draft start 和 composer submit payload 携带 selection。
- inline sendError 展示 immediate placement error 摘要，外层通知展示完整错误。
- mailbox failed row 展示 consume-time placement error 摘要，并提供完整错误查看入口。
- 观察到 queued message 异步失败时弹出外层通知，且相同失败不会重复刷屏。
- 当前默认 backend / selected backend 状态在 workspace 中可见。

## Rollback Considerations

本项目未上线，不做旧行为兼容。若实现中发现 sticky preference 字段归属不适合 lifecycle_agents，应在设计阶段改到更合适的 AgentRun read/write model 表，再统一迁移和代码。
