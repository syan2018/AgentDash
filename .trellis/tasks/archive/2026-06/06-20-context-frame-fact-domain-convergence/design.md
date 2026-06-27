# ContextFrame 事实域收束重构设计

## Architecture Boundary

ContextFrame 是运行时上下文的标准投递协议，但它不应替代事实源本身。重构后的边界如下：

- `CapabilityState` 是能力事实源。
- `SessionContextBundle` / `ContextFragment` 是任务语义片段的组装来源。
- `ProcedureContract` 是 workflow 运行合同来源，其字段按语义投影到不同事实域。
- `ContextFrame` 是事实域投递面，负责结构化 sections、模型可见 rendered text、前端调试视图和 usage 统计的统一桥接。

## Target Frame Domains

### Capability Domain

事实源：`CapabilityState`

承载内容：

- capability keys
- tool paths
- MCP servers
- VFS mounts
- tool schema
- skills
- companion roster

目标 frame：

- `capability_state_snapshot`：initial/bootstrap 或显式全量刷新。
- `capability_state_delta`：live transition 或 pending transition 的增量变化。

CAP 直接使用 frame kind 表达 snapshot / delta，原因是前端卡片、usage 统计和 runtime transition 审计都需要在 frame 层判断完整状态与增量变化。

### Assignment Domain

事实源：task/story/project context、workflow projection、`WorkflowInjectionSpec.guidance`、`WorkflowInjectionSpec.context_bindings`、声明式 context sources。

承载内容：

- task/story/project 语义摘要
- active workflow step
- workflow guidance
- context bindings 与 resolved context
- requirements / constraints / instruction

目标 frame：

- `assignment_context`

Assignment 不承载能力事实、系统偏好、项目规则全文、pending action 控制状态。

### System Guidelines Domain

事实源：用户 settings、VFS discovered guidelines。

目标 frame：

- `system_guidelines`

该 frame 是 system prompt 组成部分，同时在 timeline/debug UI 中展示。

### Runtime Control Domain

事实源：hook runtime、pending actions、auto resume、compaction。

目标 frame：

- `pending_action`
- `auto_resume`
- `compaction_summary`
- hook trace / audit event

hook runtime 可以产出 assignment 语义，但应通过 `HookInjection -> ContextFragment -> assignment_context` 的标准路径进入模型，不再保留独立 hook injection frame 作为第二投递面。

## ProcedureContract Projection

`AgentProcedureContract` 字段的目标落点：

- `capability_config.tool_directives` / `mount_directives` -> capability resolver -> `CapabilityState` -> CAP frame。
- `injection.guidance` -> assignment frame。
- `injection.context_bindings` -> assignment frame。
- `hook_rules` -> hook runtime / pending action / trace。
- `input_ports` / `output_ports` -> workflow/task delivery section，作为任务合同表达；如需影响能力面，必须通过 capability resolver。

## Known Residuals To Resolve

- `project_guidelines` assignment slot 与 `system_guidelines` 的重叠。
- `runtime_policy` fragment 中混入 capability keys，且 scope 表达 RuntimeAgent 可见。
- `bootstrap_context` 注释和 `bootstrap_fragments` 命名。
- `context_usage_items_from_section` 未覆盖 CAP delta dimensions。

## Data Flow Targets

### Companion Roster

`ProjectAgent config / runtime mutation intent`
-> `CapabilityResolver` or explicit capability replay
-> `CapabilityState.companion.agents`
-> `AgentFrame.effective_capability_json`
-> `ExecutionContext.turn.capability_state`
-> CAP snapshot/delta section
-> frontend CAP card and `companion_request` tool.

### Workflow Assignment

`AgentProcedureContract.injection`
-> workflow projection / context binding resolver
-> `ContextFragment`
-> `assignment_context` section
-> rendered text and context usage item.

### System Guidelines

`settings + discovered AGENTS.md/MEMORY.md`
-> `system_guidelines` section
-> stable system prompt assembly and frontend debug view.

## Trade-Offs

- Splitting CAP snapshot/delta increases protocol surface but makes UI semantics correct.
- Removing legacy section kinds reduces drift but requires updating generated samples, tests and frontend parser fixtures together.
- Keeping `ContextFrame.rendered_text` as model delivery requires usage statistics to stay section-complete.

## Migration Notes

Project is prelaunch, so the implementation should converge directly to the target contract. Database migration is only needed if persisted event/session payload shape must be read by new code; otherwise persisted historical payload compatibility is not a blocker.

## Work Item Boundaries

Work items are tracked inside `work-items.md` under this task. They are not independent Trellis tasks.

1. Protocol taxonomy owns the frame/section vocabulary and deletion/redefinition decisions.
2. CAP convergence owns capability fact projection, snapshot/delta behavior and companion roster route.
3. Assignment convergence owns task semantics and ProcedureContract projection.
4. Delivery/usage convergence owns rendered_text, model delivery channels and usage accounting.
5. Frontend convergence owns parser/render surface and user-facing diagnostics.
6. Integration/spec verification owns final validation and Trellis spec updates.

Implementation should follow the work item order unless an earlier investigation changes the dependency graph.
