# W6 MCP + Capability

## 状态

done

## 依赖

- W4 done

## 目标

收口 Story / Task MCP 与 capability，使 agent-facing 工具走 Run-scoped Task command 和 SubjectExecution projection，并保留默认开放的 policy hook。

## 输入

- W4 backend command / read model。
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/permission/grant-lifecycle.md`
- `.trellis/spec/backend/permission/policy-engine.md`
- `crates/agentdash-mcp/src/servers/story.rs`
- `crates/agentdash-mcp/src/servers/task.rs`
- `crates/agentdash-spi/src/platform/tool_capability.rs`
- `crates/agentdash-application/src/capability/resolver.rs`
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md`

## 范围

- Story MCP 通过 Story-bound run 的 Task command 创建计划项，或查询 Story Task projection。
- Task MCP 状态推进只接受计划态。
- artifact 上报不写 Task facts，改走 Lifecycle / SubjectExecution 关联产物。
- Task management、collaboration、workflow capability 调用统一 command / read model。
- 为 create / update / assign / review / done 预留 policy check，默认开放。
- Story projection read / update 保留 Story scope capability check。

## 范围边界

- 该节点只预留稳定 policy hook 并保持默认开放，原因是 permission system convergence review 会独立收束完整策略。
- MCP 工具统一调用 Run-scoped Task command，原因是 agent-facing 入口需要和 API / UI 观察同一事实源。

## 验收

- Story / Task MCP 写入口走 Run-scoped Task command。
- Task MCP 拒绝旧 TaskStatus。
- artifact 不再写入 Task facts。
- Task / Story 相关入口存在稳定 policy hook。
- 默认行为不阻塞预研开发。

## 产出记录

- `agentdash-mcp` 现在依赖 application command 层；Story / Relay MCP 的 Task 查询走 `build_story_task_projection`，不再读取 Story-owned `tasks`。
- Story MCP `create_task` / `batch_create_tasks` 要求传入 Story-bound `run_id`，通过 `create_run_task` 写入 `LifecycleRun.tasks`，并使用 `story_ref` 作为 projection hint。
- Task MCP `update_task_status` 只接受 `open / active / review / blocked / done / dropped`，通过 `transition_run_task_status` 推进计划状态；`append_task_description` 通过 `update_run_task` 修改 plan body。
- Task MCP `report_artifact` 不写 Task facts，不使用旧 Task artifact type；仅记录 `subject_execution_artifact_reported` state_change 事件，携带可选 artifact path / content 作为执行投影关联线索。
- `TaskPlanPolicyHook` 已在 run-scoped create / update / assign / review / done / archive 写入口预留，当前默认开放，后续 permission convergence 可接管该函数。
- `CapabilityScopeCtx::Task` 的 Story 归属改为 `Option<Uuid>`；Task MCP 注入只要求 task id，resolver、SubjectContextAssignment 与 activity activation 测试已同步。
- companion-system skill 增加 Task plan tools 说明：companion completion 通过 `artifact_refs` / SubjectExecution-linked paths 返回执行证据，原因是 Task facts 只描述计划进度。

## 风险与交接

- W8 可使用本节点搜索结果继续总清理：W6 范围内 `crates/agentdash-mcp`、`crates/agentdash-application/src/capability`、`crates/agentdash-spi/src/platform` 已无 `dispatch_preference`、`story.tasks`、旧 TaskStatus 描述、`TaskArtifactAdded` 或旧 `ArtifactType` surface。
- Relay MCP 仍允许 Story 状态 `completed / failed / cancelled`，这是 Story workflow 状态语言，不属于旧 TaskStatus surface。
- `report_artifact` 当前只记录 SubjectExecution 关联事件，尚未接入专用 artifact repository；后续如需要可由 SubjectExecution / lifecycle artifact 任务把该 state_change 事件替换为正式产物命令。
