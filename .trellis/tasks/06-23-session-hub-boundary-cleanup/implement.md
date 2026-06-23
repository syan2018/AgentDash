# 清理 session hub 错误归属路径实施计划

## Phase 0: Research And Boundary Lock

- [x] 收集子代理研究结果：
  - [x] `research/session-hub-inventory.md`
  - [x] `research/runtime-gateway-mcp-boundary.md`
  - [x] `research/business-consumer-migration-map.md`
  - [x] `research/agentframe-exposure-inventory.md`
- [x] 更新 `design.md` 的迁移矩阵，补齐核心旧路径、调用点、目标归属和测试。
- [x] 根据 `AgentFrame` 暴露面清单标注允许直接持有 frame 的内聚实现区域，以及必须迁到 query/update/read-model facade 的裸引用。
- [x] 决定 query facade / query port 最终命名与模块归属：
  - `agent_run::runtime_surface::AgentRunRuntimeSurfaceQuery`
  - `AgentRunRuntimeSurfaceQueryPort`
  - public result 使用 `AgentRunRuntimeSurface` / `CurrentRuntimeSurface` 风格结构
- [x] 决定 active turn runtime action query 的事实源策略：
  - AgentRun/Lifecycle current committed runtime surface + active turn transient metadata
  - connector live tool refresh 继续使用 hub active turn snapshot
- [x] 决定 runtime backend anchor 派生逻辑的共享方式：
  - 提取 VFS -> `RuntimeBackendAnchor` helper
  - query facade 内部可用 `ClosedRuntimeSurface::from_revision(...)`
  - 不向 query consumer 暴露 `FrameLaunchSurface`

## Phase 1: AgentRun Runtime Surface Query

- [ ] 新增 AgentRun/Lifecycle runtime surface query facade 与窄 query port。
- [ ] 从 `RuntimeSessionExecutionAnchor` 解析 run / agent / runtime address。
- [ ] 在 query facade 内部读取 current surface revision，并闭合 typed capability / VFS / MCP / execution metadata。
- [ ] 从 closed VFS surface 派生可选 `RuntimeBackendAnchor`，并提供 backend-required query/helper 供 MCP、Extension runtime、Terminal 等执行路径使用。
- [ ] 保留 project/run/agent/runtime address/surface revision provenance 与 typed error。
- [ ] 将 `resolve_current_frame_from_delivery_trace_ref` 降级为 query facade 内部实现 helper，或至少停止让 RuntimeGateway/API current-surface consumer 直接调用。
- [ ] 新 query port 的 public result 不包含 `AgentFrame`、`AgentFrameRepository` 或 `FrameLaunchSurface`。
- [ ] 增加 query facade 单元测试：
  - [ ] runtime session 缺 anchor。
  - [ ] anchor 指向 agent/run 不一致。
  - [ ] current surface revision 缺 VFS/capability/MCP closure。
  - [ ] default mount backend id 生成 backend anchor。
  - [ ] workspace metadata 生成 workspace binding anchor。
  - [ ] resource query 在缺 backend anchor 时仍返回 closed resource surface。
  - [ ] backend-required query 在缺 backend anchor 时返回带 purpose 的 typed error。

## Phase 2: RuntimeGateway MCP Access Migration

- [ ] 新增 `RuntimeSessionMcpAccess` 实现，依赖 AgentRun runtime surface query port 与 MCP discovery port。
- [ ] `mcp.list_tools` 通过 query facade 返回的 surface 构造 `McpToolDiscoveryRequest`。
- [ ] `mcp.call_tool` 复用同一 discovery entries 并执行目标 tool。
- [ ] AppState/bootstrap 改为注入新的 MCP access，而不是 `SessionCapabilityService`。
- [ ] 删除 `SessionCapabilityService impl RuntimeSessionMcpAccess`。
- [ ] 从 `session/hub/tool_builder.rs` 移除 runtime action backing discovery 的 idle surface fallback。
- [ ] 保留 hub 内 active turn connector tool refresh 逻辑，并确保它不承担 RuntimeGateway session action backing access。
- [ ] 增加 RuntimeGateway/MCP 测试：
  - [ ] Canvas/user session actor 的 `mcp.list_tools` 能在 idle session 下拿到 backend anchor。
  - [ ] `mcp.call_tool` 的 runtime_name / server_name + tool_name 匹配行为保持。
  - [ ] capability disabled tool 不暴露。

## Phase 3: API Current Surface Consumers

- [ ] 用 AgentRun runtime surface query 替换或改造 `resolve_session_frame_vfs`。
- [ ] 删除 `SessionFrameVfsResult.frame` 这类向 API current-surface consumer 泄漏 `AgentFrame` 的字段，或将 helper 私有化为 query adapter 内部实现。
- [ ] Canvas runtime snapshot / binding resource path 改为消费 query facade VFS。
- [ ] Extension runtime action/channel target 解析改为消费 query facade backend anchor + VFS workspace context。
- [ ] VFS surface `SessionRuntime` source 改为消费 query facade。
- [ ] VFS surface `AgentRun` current delivery source 改为消费同一 resource surface facade，不再在 API route 内独立执行 current frame + lifecycle projection。
- [ ] Terminal spawn / launch target 改为消费 query facade 的 backend anchor + VFS。
- [ ] 检查 terminal/session VFS target 相关路径是否仍依赖 active-turn-only backend anchor helper。
- [ ] 保留 API route 的 project permission 校验，并明确权限校验发生在 API adapter 还是 query facade。
- [ ] 对 `crates/agentdash-api/src` 增加 focused import review：Canvas/Extension/VFS/Terminal/runtime action 路径不再直接 import `AgentFrame`、`AgentFrameSurfaceExt` 或 current frame resolver；presentation read-model 除外。
- [ ] 增加 API/route 测试：
  - [ ] Canvas runtime snapshot 使用同一 current surface VFS。
  - [ ] Extension runtime invoke 使用 query facade backend target。
  - [ ] SessionRuntime VFS surface 与 Canvas snapshot 看到同一 VFS/default mount。
  - [ ] VFS `AgentRun` source 与 `SessionRuntime` source 对同一 delivery runtime 使用同一 resource surface policy，生命周期 evidence projection 是否叠加由 facade 统一决定。

## Phase 4: Business Update / Adoption Old Path Cleanup

- [ ] 调查并迁移 Canvas exposure/adoption helper。
- [ ] 调查并迁移 WorkspaceModule Canvas operation 后的 runtime surface update path。
- [ ] 调查并迁移 Permission grant apply/revoke active runtime adoption path。
- [ ] 将 active runtime adoption primitive 收窄为 runtime surface update use case 内部 helper。
- [ ] 删除、私有化或重命名误导性 facade：
  - [ ] `SessionCapabilityService::expose_canvas_mount_revision_and_adopt`
  - [ ] `SessionCapabilityService::adopt_persisted_agent_frame_revision`
  - [ ] `SessionCapabilityService::resolve_runtime_session_target`
  - [ ] active-turn-only `get_current_runtime_backend_anchor` 对 API/业务的暴露
- [ ] 增加静态 grep 检查或 focused tests，确保业务模块不直接调用旧 adoption primitive。

## Phase 5: Spec And Cleanup

- [ ] 更新 `.trellis/spec/backend/runtime-gateway.md`：MCP session action backing access 消费 AgentRun runtime surface query port。
- [ ] 更新 `.trellis/spec/backend/session/runtime-execution-state.md`：`session/hub` 只表达 live runtime coordination，不表达 AgentRun current surface query。
- [ ] 如需要，新增 backend appendix 记录 AgentRun runtime surface query contract 与 `AgentFrame` exposure boundary。
- [ ] 清理 task research 中已转入 design 的临时 TODO。
- [ ] 跑质量检查。

## Efficient Parallel Dispatch Plan

Implementation should use Trellis sub-agents after `task.py start`. The main session coordinates interfaces, reviews returned patches, resolves merge conflicts, runs final checks, updates specs, and commits.

### Serial Foundation

Dispatch first as one focused `trellis-implement` worker because later slices depend on the new port and result type.

```text
Worker Foundation: AgentRun runtime surface query
Owns:
- crates/agentdash-application/src/agent_run/runtime_surface*
- narrow lifecycle/helper changes needed by the query facade
- shared VFS -> RuntimeBackendAnchor helper

Must not touch:
- API route migrations
- RuntimeGateway provider wiring
- business adoption cleanup

Output:
- AgentRunRuntimeSurfaceQuery / AgentRunRuntimeSurfaceQueryPort
- public result without AgentFrame / FrameLaunchSurface
- resource-surface result supports optional backend anchor, while backend-required helper returns typed missing-anchor error
- unit tests for idle/current surface closure
```

### Parallel Round 1

After the foundation port compiles, dispatch these workers in parallel because their write sets are mostly disjoint.

```text
Worker A: RuntimeGateway MCP access migration
Owns:
- crates/agentdash-application/src/runtime_gateway/*
- crates/agentdash-api/src/bootstrap/runtime_gateway.rs
- crates/agentdash-api/src/app_state.rs
- RuntimeSessionMcpAccess implementation
- the MCP-discovery portions of crates/agentdash-application/src/session/capability_service.rs
- the MCP action backing / idle discovery portions of crates/agentdash-application/src/session/hub/tool_builder.rs

Must not touch:
- Canvas/Extension/VFS/Terminal API route consumer migration
- Canvas/WorkspaceModule/Permission business update paths

Output:
- SessionCapabilityService no longer implements RuntimeSessionMcpAccess
- RuntimeGateway MCP access consumes AgentRunRuntimeSurfaceQueryPort
- idle Canvas mcp.list_tools regression coverage
```

```text
Worker B: API current surface consumers
Owns:
- crates/agentdash-api/src/session_construction.rs
- crates/agentdash-api/src/routes/canvases.rs
- crates/agentdash-api/src/routes/extension_runtime.rs
- crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs
- crates/agentdash-api/src/routes/terminals.rs

Must not touch:
- RuntimeGateway MCP provider or AppState MCP wiring
- Canvas/WorkspaceModule/Permission surface update/adoption code
- Lifecycle/session presentation read-model endpoints except allowlist documentation

Output:
- resolve_session_frame_vfs replaced or reduced to a thin query adapter
- SessionFrameVfsResult no longer exposes AgentFrame
- Canvas/Extension/VFS SessionRuntime/VFS AgentRun/Terminal consume the same current/resource surface result
```

```text
Worker C: Boundary checks and spec updates
Owns:
- .trellis/spec/backend/runtime-gateway.md
- .trellis/spec/backend/session/runtime-execution-state.md
- optional backend appendix for AgentRun runtime surface query / AgentFrame exposure boundary
- focused grep/check script or documented command if a script is too heavy

Must not touch:
- production Rust code

Output:
- specs describe why current runtime surface query is not session/hub
- static import boundary checklist for AgentFrame / AgentFrameSurfaceExt / current frame resolver
```

### Parallel Round 2

Dispatch after Worker A finishes, because it shares `SessionCapabilityService` / hub adoption surfaces with the business cleanup.

```text
Worker D: Business update and adoption cleanup
Owns:
- crates/agentdash-application/src/canvas/tools.rs
- crates/agentdash-application/src/workspace_module/tools.rs
- crates/agentdash-application/src/permission/service.rs
- crates/agentdash-api/src/routes/permission_grants.rs
- remaining non-MCP adoption helpers in crates/agentdash-application/src/session/capability_service.rs
- remaining adoption primitive visibility in crates/agentdash-application/src/session/hub/tool_builder.rs

Must not touch:
- RuntimeGateway MCP provider wiring already owned by Worker A
- API current surface route migration already owned by Worker B

Output:
- route/business modules no longer directly call active runtime adoption primitive
- surface-changing effects enter runtime surface update use case
- active adoption primitive is private/internal to live runtime coordination
```

```text
Worker E: Integration check
Owns:
- no production feature work
- targeted tests, cargo check, grep boundary checks

Runs after A/B/C and can overlap with D if D only changes business mutation paths:
- cargo test -p agentdash-application runtime_gateway
- cargo test -p agentdash-api canvases
- cargo test -p agentdash-api extension_runtime
- cargo test -p agentdash-api vfs_surfaces
- cargo check -p agentdash-application
- cargo check -p agentdash-api
```

### Main Session Integration Rules

- Do not run Worker A and Worker D at the same time; they both touch `SessionCapabilityService` and hub adoption surfaces.
- Worker B can run in parallel with A because it owns API current-surface routes and should only consume the new query port.
- Worker C can run in parallel with A/B because it is docs/spec/checklist only.
- If Worker D becomes large, split it into:
  - D1 Canvas + WorkspaceModule surface update
  - D2 Permission grant adoption
  These may run in parallel only if D1 does not touch `permission/service.rs` or `routes/permission_grants.rs`, and D2 does not touch Canvas/WorkspaceModule files.
- Presentation/read-model frame DTO cleanup is not part of this dispatch plan unless it blocks import boundary checks; add an allowlist instead.

## Validation Commands

初始候选：

```powershell
cargo test -p agentdash-application runtime_gateway::session_actions
cargo test -p agentdash-application session::hub
cargo test -p agentdash-api canvases
cargo test -p agentdash-api extension_runtime
cargo test -p agentdash-api vfs_surfaces
cargo check -p agentdash-application
cargo check -p agentdash-api
```

根据研究结果和实际改动范围再缩小或扩展。

## Rollback Points

- Phase 1 query facade 新增后可先不切换 consumer；若测试失败，可单独回滚 query module。
- Phase 2 RuntimeGateway MCP access 切换是第一处行为切换，必须在提交前有 MCP list/call 回归测试。
- Phase 3 API consumer 迁移会影响 Canvas、Extension runtime 和 VFS surface，应分模块提交或至少分阶段验证。
- Phase 4 adoption cleanup 涉及业务 mutation，风险最高；如果 Phase 2/3 已解决 Canvas MCP failure，可将 Phase 4 拆成后续子任务，但必须保留设计矩阵和旧入口禁用计划。
- `AgentFrame` 裸引用收口不要一次性扫完整个 application；优先迁 RuntimeGateway、API current-surface consumer、session hub idle query 与 business update/adoption。Lifecycle/session presentation read-model 可作为后续 crate split/read-model cleanup。

## Review Gate Before Start

- [x] 用户确认 `session/hub` 保留职责和迁出职责。
- [x] 用户确认 query facade / query port 命名与归属。
- [x] 用户确认 `AgentFrame` exposure boundary：本任务硬收 RuntimeGateway/API current-surface/business update，presentation read-model 可后续处理。
- [x] 子代理 research 已回填到 `design.md`。
- [x] `implement.jsonl` / `check.jsonl` 已补充关键 spec 与代码上下文。
- [ ] 当前任务经 `task.py start` 激活后再进入代码实现。
