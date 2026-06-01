# 执行计划

## 顺序

1. **实现 `LifecycleGate` companion wait/resume 路径**
   - 在 `LifecycleGateRepository` 上实现 `create_companion_gate(run_id, parent_agent_id, correlation, adoption_mode)` 便捷方法。
   - gate kind 枚举新增 `companion_wait`；payload 包含 `parent_agent_id`、`adoption_mode`、`slice_mode`、`request_payload`。
   - 实现 `resolve_gate(gate_id, resolution_payload)` → 更新 gate status + 触发 parent agent resume。

2. **实现 `AgentLineage` companion 关系**
   - companion dispatch 时写入 `AgentLineage { parent_agent_id, child_agent_id, relation=companion, run_id }`。
   - 提供 `list_children(parent_agent_id)` 和 `find_parent(child_agent_id)` 查询。

3. **重写 `CompanionRequestTool` dispatch 路径**
   - `companion_request(target=sub)` → 构造 `ExecutionIntent { parent_agent_id, gate_policy=companion_wait, agent_policy=spawn_child, context_policy=inherit/slice }`。
   - 调用 `LifecycleDispatchService::dispatch()` → 获取 child agent ref + gate ref。
   - 将 child agent ref / gate ref 存入 tool result，不再存 session_id。

4. **替换 `CompanionSessionContext` 为 frame-based context**
   - companion child 的 `AgentFrame.context_slice` 从 parent `AgentFrame` 按 `SliceMode` 投影。
   - 删除 `CompanionSessionContext` 在 session construction plan 中的使用。
   - 删除 `SessionMeta.companion_context` 字段的写入路径。

5. **实现 gate → hook pending action 桥接**
   - gate resolution 后产生 `AgentFrameEvent(kind=companion_result, payload=adoption_result)`。
   - hook runtime 从 `AgentFrameEvent` 读取 companion_result trigger，不再从 session metadata。
   - `CompanionAdoptionMode` 映射：`suggestion → auto_apply`、`follow_up_required → pending_review`、`blocking_review → blocking_gate`。

6. **实现 companion resume 路径**
   - gate resolved → parent `LifecycleAgent` 收到 resume signal。
   - parent AgentFrame 根据 adoption_mode 更新 context slice / pending action。
   - 若 `blocking_review`：parent agent execution 被挂起直到 gate resolve；resume 后继续。
   - 若 `suggestion`：结果直接注入 parent frame context，不阻塞。

7. **更新 `CompanionRequestTool` 路由为 agent-based**
   - `current_session_id` 替换为 `current_agent_id` / `current_frame_id`。
   - `active_mcp_servers()` 通过 frame 投影获取，不再直接查 session runtime。
   - tool 结果返回 `agent_ref` / `gate_ref`，不返回 `session_id`。

8. **删除 SessionMeta companion 事实源**
   - 删除 `SessionMeta.companion_context` 字段读写。
   - 删除 session construction 中 companion context injection 的旧路径。
   - 删除 session lineage 中 companion kind 作为 ownership 推断源的使用。

9. **更新 `RuntimeSessionLineage` 保留 trace 语义**
   - companion child session 仍写入 `RuntimeSessionLineage(kind=companion, parent_session_id)`，但只用于 trace。
   - agent ownership 查询使用 `AgentLineage`。

## 质量门

- `SessionMeta.companion_context` 不再被写入或读取为控制面事实源。
- companion resume 可以在进程重启后从 `LifecycleGate` 恢复。
- companion child 的业务归属通过 `AgentLineage` + `LifecycleSubjectAssociation` 查询。
- companion graph 不因"是子图"而创建 child `LifecycleRun`。
- `CompanionRequestTool` 不通过 `session_id` 路由 companion dispatch。

## 验证命令

- `python ./.trellis/scripts/task.py validate 06-01-companion-gate-lineage-migration`
- `cargo build -p agentdash-application -p agentdash-api`
- `rg -n "companion_context|CompanionSessionContext|CompanionLaunchSource" crates/agentdash-application/src/`
- `rg -n "current_session_id" crates/agentdash-application/src/companion/`
- `git diff --check -- .trellis/tasks`

## 后续交接

- `frontend-actor-subject-views` 将前端 companion 面板从 session lineage 切到 `AgentLineage` + `LifecycleGate` 视图。
- `session-first-api-demotion` 删除 companion 相关的 session-first API response fields。
- `routine-run-source-migration` 的 routine companion（如果有）可复用同一 gate/lineage 路径。
