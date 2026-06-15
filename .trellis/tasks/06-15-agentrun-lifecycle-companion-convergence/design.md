# Design

## Objective

Agent-facing runtime surface 应以 AgentRun 为中心构造。RuntimeSession 只表达 delivery trace/ref，不能决定业务 owner、context、capability、VFS 或 SkillAsset 可见性。

本设计把 companion 协议与 embedded skill bundle projection 合并到同一套 AgentRun lifecycle surface：

```text
AgentRun run/agent/frame/anchor
  -> Frame runtime surface
  -> AgentRun lifecycle VFS mount
  -> Project SkillAsset projection
  -> skill baseline / connector-visible VFS / frontend resource surface
```

## Companion Payload Contract

`companion_request` 外层 envelope 保持：

```json
{
  "target": "sub",
  "wait": true,
  "payload": {}
}
```

`payload.message` 是 request 正文的唯一标准字段。

### Request Payload Matrix

| target | payload.type | Required fields | Response type | Purpose |
| --- | --- | --- | --- | --- |
| `sub` | `task` | `message` | `completion` | 派发 companion child agent 执行任务 |
| `parent` | `review` | `message` | `resolution` | child 向 parent 提交审阅或决策请求 |
| `human` | `approval` | `message` | `decision` | 请求用户选择、批准或补充信息 |
| `human` | `notification` | `message` | none | 向用户发送无需阻塞的通知 |
| `platform` | `capability_grant_request` | `requested_paths`, `reason`, `scope` | `capability_grant_result` | 请求平台评估临时能力授权 |

### Response Payload Matrix

| payload.type | Required fields | Used for |
| --- | --- | --- |
| `completion` | `status`, `summary` | sub task 完成结果 |
| `resolution` | `status`, `summary` | parent review 结论 |
| `decision` | `choice` | human approval/choice |
| `capability_grant_result` | `status`, `summary` | platform grant broker 结果 |

### Tool Schema

`CompanionRequestParams.payload` 不应继续暴露为完全开放 object。schema 应表达已注册 request payload 的 `anyOf`，并允许未来扩展通过明确 `type` 注册进入。

`companion_request` description 应说明：

- request 正文字段使用 `payload.message`。
- `payload.prompt` 不属于 companion request contract。
- `notification` 与其它交互正文同样使用 `message`。
- `capability_grant_request` 使用结构化 grant 字段。

## Embedded Skill Bundle Projection

内嵌 bundle 的事实链路保持三段：

```text
EmbeddedSkillBundle
  -> SkillAssetService::bootstrap_builtins(project_id, Some(key))
  -> Project SkillAsset
  -> AgentRun lifecycle VFS metadata
```

Agent 不直接读取 embedded bundle。执行器和前端都通过 runtime VFS / capability projection 发现 skill。

### Builtin Skill Keys

| Key | Projection trigger | Reason |
| --- | --- | --- |
| `companion-system` | AgentRun runtime surface 具备 collaboration/companion capability | Agent 需要知道 companion_request/companion_respond 的协议 |
| `workspace-module-system` | AgentRun runtime surface 具备 workspace_module capability | Agent 需要知道 workspace module create/list/describe/invoke/present 顺序 |
| `routine-memory` | Routine-owned AgentRun / routine frame construction | Routine memory 协议属于 routine runtime context |

## AgentRun Lifecycle VFS Model

AgentRun runtime surface 应稳定持有 AgentRun lifecycle VFS mount。当前代码里的 `node_runtime` 与 `agent_run_session` 两类 scope 需要收束为一个不会互相覆盖 metadata 的模型。

推荐模型：

```text
lifecycle://run/{run_id}/agent/{agent_id}/session/{runtime_session_id}/
  state
  session/
    items
    messages
    tools
    writes
    summaries
    terminal
    turns
  node/
    state
    artifacts/
    records/
  skills/
    companion-system/
    workspace-module-system/
    routine-memory/
```

同一个 `lifecycle` mount 承载 AgentRun session root。若当前 anchor 有 orchestration node，则 `node/` subtree 暴露 node artifact/record surface；若没有 node anchor，则 `node/` 为空或只返回状态说明。

metadata 必须包含：

```json
{
  "scope": "agent_run_session",
  "run_id": "...",
  "agent_id": "...",
  "runtime_session_id": "...",
  "launch_frame_id": "...",
  "orchestration_id": "...",
  "node_path": "...",
  "attempt": 1,
  "skill_asset_project_id": "...",
  "skill_asset_keys": ["companion-system"]
}
```

### Merge Rule

构造 AgentRun lifecycle mount 时必须保留或重新计算 skill projection metadata。替换旧 lifecycle mount 时不能丢失 `skill_asset_project_id` / `skill_asset_keys`。

推荐提供单一 helper：

```rust
build_agent_run_lifecycle_vfs_with_skills(
    base_vfs,
    anchor,
    project_id,
    skill_asset_keys,
)
```

该 helper 负责：

- 移除旧 `id="lifecycle"` mount。
- 创建 AgentRun session-scope lifecycle mount。
- 写入 anchor metadata。
- 合并显式 agent skill keys 与 builtin system skill keys。
- 写入 skill projection metadata。
- 归一化 default mount。

## Frame Construction Paths

### ProjectAgent Graphless

ProjectAgent 没有 active workflow 时仍然属于 AgentRun。首轮 owner bootstrap frame construction 必须安装 AgentRun lifecycle mount，并投影 `companion-system` 与 agent preset skill keys。

如果 capability 包含 `workspace_module`，同一 mount 上追加 `workspace-module-system`。

### Workflow Node

Workflow node frame construction 使用同一 AgentRun lifecycle mount。node artifact/record capability 由 anchor 的 orchestration/node metadata 决定，而不是通过另一种 mount scope 覆盖。

### Plain Companion Child

Plain companion child 不能只继承 parent VFS slice。child AgentRun 有自己的 run/agent/frame/runtime anchor，因此 frame construction 应在 parent slice 基础上安装 child AgentRun lifecycle mount，并投影 `companion-system`。

### Companion + Workflow Child

Companion + workflow child 继续叠加 workflow activation，但 lifecycle surface 也应落到同一 AgentRun lifecycle mount。workflow node artifact/record 作为 `node/` subtree 暴露。

### Workspace Query

Workspace query 不应临时构造一个丢失 skill metadata 的 lifecycle mount 覆盖 frame typed VFS。查询侧应使用与执行侧相同的 AgentRun lifecycle VFS helper，或直接读取 frame runtime surface 中已经闭包后的 VFS。

## Capability And Frontend Projection

`derive_session_skill_baseline` / `load_skills_from_vfs` 应从最终 AgentRun lifecycle VFS 发现 projected skills。前端 capability card 和 resource surface 消费同一 projection，不单独推导 builtin skill 可见性。

前端可见性来自：

```text
AgentFrame runtime surface
  -> effective VFS
  -> lifecycle skills/
  -> SessionBaselineCapabilities.skill_clusters / skills
```

## Trade-offs

### Single `lifecycle` mount vs separate node mount

推荐单一 `lifecycle` mount，因为 AgentRun workspace、connector VFS、skill discovery 和前端 resource surface 都能消费同一个事实源。

node artifact/record 使用 `node/` subtree 表达，避免两个 `id="lifecycle"` mount 互相覆盖 metadata。

### Direct embedded bundle exposure vs Project SkillAsset projection

推荐继续经 Project SkillAsset 投影，因为这保持 embedded bundle、项目资产管理、runtime VFS、skill baseline 的事实源一致。

### `message` vs `prompt`

推荐 `message`。Companion request 是跨主体交互消息，不是 session launch prompt。统一 `message` 能减少模型误填，也让 human/parent/sub/platform 语义一致。

## Spec Updates

实现阶段应同步更新：

- `.trellis/spec/backend/embedded-skill-bundles.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- companion-system skill references
- 如涉及 VFS contract，更新相关 VFS spec
