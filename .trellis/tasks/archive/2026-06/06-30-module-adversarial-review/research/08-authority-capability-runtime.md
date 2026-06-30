# Research: Authority & Capability Runtime

- Query: 单域对抗性架构审查：PermissionGrant / policy / escalation / CapabilityResolver / tool catalog / MCP capability / VFS capability；Contract 仅作为投影证据；重点检查授权事实源与运行时 capability state 边界、可用性与授权是否重复表达，并对照 06-14 baseline。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `.trellis/tasks/06-30-module-adversarial-review/check.jsonl` - 当前任务检查记录与审查约束。
- `.trellis/tasks/06-30-module-adversarial-review/prd.md` - 当前对抗性架构审查目标与输出要求。
- `.trellis/tasks/06-30-module-adversarial-review/design.md` - 当前审查设计范围。
- `.trellis/tasks/06-30-module-adversarial-review/implement.md` - 当前审查执行计划。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 06-14 baseline 主报告。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` - 06-14 VFS / relay / extension baseline。
- `.trellis/tasks/06-14-module-overdesign-review/research/04-frontend-contracts-permission.md` - 06-14 frontend contract / permission baseline。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - 06-14 AgentRun / session runtime baseline。
- `.trellis/spec/backend/capability/architecture.md` - capability resolver、tool schema exposure、tool admission 的边界规范。
- `.trellis/spec/backend/permission/architecture.md` - `PermissionGrant` 事实源、policy pure function、runtime adoption 规范。
- `.trellis/spec/backend/vfs/architecture.md` - runtime tool composer 与 VFS capability provider 规范。
- `.trellis/spec/backend/architecture.md` - backend 分层与依赖边界。
- `.trellis/spec/cross-layer/architecture.md` - Contract-first 与跨层投影规范。
- `crates/agentdash-domain/src/permission/entity.rs` - `PermissionGrant` 授权实体与作用 frame 字段。
- `crates/agentdash-domain/src/permission/repository.rs` - grant repository 的 frame/run 查询口。
- `crates/agentdash-domain/src/permission/value_objects.rs` - grant active 状态定义。
- `crates/agentdash-application/src/permission/service.rs` - grant request / approve / revoke / expire 与 runtime surface update。
- `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs` - grant 到 AgentFrame surface revision 的应用边界。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - AgentRun effective view、grant projection、runtime execution capability state。
- `crates/agentdash-application-ports/src/agent_run_surface.rs` - AgentRun effective capability / admission port contract。
- `crates/agentdash-application-ports/src/runtime_session_live.rs` - runtime session live port contract。
- `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs` - runtime session 工具装配前的 capability state projection。
- `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs` - runtime provider 与 MCP discovery 装配。
- `crates/agentdash-application/src/capability/resolver.rs` - declarative capability resolver、MCP expansion、tool policy 编译。
- `crates/agentdash-application/src/capability/tool_catalog.rs` - 后端 capability/tool catalog 读模型。
- `crates/agentdash-api/src/routes/permission_grants.rs` - permission grant API contract 投影。
- `crates/agentdash-api/src/routes/workflows.rs` - workflow tool catalog API contract 投影。
- `crates/agentdash-contracts/src/system/permission.rs` - generated permission contract 源。
- `crates/agentdash-contracts/src/runtime/workflow.rs` - generated workflow/catalog contract 源。
- `packages/app-web/src/generated/permission-contracts.ts` - permission contract 前端生成物投影。
- `packages/app-web/src/generated/workflow-contracts.ts` - workflow/catalog contract 前端生成物投影。
- `packages/app-web/src/features/permission/PermissionGrantCard.tsx` - grant UI 对 scope / policy / escalation 的消费。
- `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx` - capability panel catalog 消费。
- `packages/app-web/src/services/workflow.ts` - catalog API client。
- `packages/app-web/src/types/workflow.ts` - workflow 类型别名到 generated contract。
- `crates/agentdash-application/src/companion/payload_types.rs` - companion capability grant payload registry。
- `crates/agentdash-application/src/companion/tools.rs` - companion capability grant tool schema 与 unsupported broker error。
- `packages/app-web/src/features/session/model/companionRequestViewModel.ts` - companion capability grant UI view model。

### Baseline comparison

- 06-14 P0 `PermissionGrant` 与 companion capability grant 双事实源：当前 backend 在 `crates/agentdash-application/src/companion/tools.rs:2006` 明确报 `platform_capability_grant_missing_broker_error`，说明 platform-targeted companion grant 没有落到另一套 active 授权事实源；P0 双事实源已降级为“残留授权协议投影”问题。
- 06-14 P1 permission contract typed gap：当前 `crates/agentdash-contracts/src/system/permission.rs:44`、`:51`、`:73` 定义了 typed `PolicyDecisionDto`、`ScopeEscalationIntentDto`、`PermissionGrantResponse`；`packages/app-web/src/generated/permission-contracts.ts:6` 已生成 typed projection；该项已收束。
- 06-14 P1 active-only permission list：当前 `crates/agentdash-contracts/src/system/permission.rs:57` 定义 `ListPermissionGrantsQuery` 的 `status` / `status_group`；`crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:217` 编译 status filter；该项已收束。
- 06-14 P1 frontend 手写 capability/tool catalog 镜像：当前 `crates/agentdash-contracts/src/runtime/workflow.rs:71`、`:92` 定义 `ToolDescriptorDto` / `CapabilityCatalogResponse`，`packages/app-web/src/types/workflow.ts:198` 直接 alias generated `ToolDescriptorDto`；该项已收束。
- 06-14 runtime provider 过宽的 relay composer：当前 `crates/agentdash-api/src/bootstrap/session.rs:410` 组装 `SessionRuntimeToolComposer` 与 VFS / workflow / collaboration / task / workspace module providers，VFS provider 独立在 `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:15`；该项已收束。

### Issue 1 - Admission-only grant 被重新写回 CapabilityState，导致模型可见能力与执行准入重复表达

- Priority: P0
- Category: authorization boundary violation / duplicated availability-authority expression
- Code evidence:
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:37` 注释声明“只有工具级 Grant 会进入这里作为工具执行准入；能力级 / MCP server 级 Grant 会写入新的 AgentFrame revision，并由 frame capability surface 表达模型可见效果”。
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:71` 将带 `tool` 的 path 分类为 `AdmissionProjection`，无 tool 的 path 分类为 `AgentFrameSurfaceRevision`。
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:116` 的 `apply_to_execution_capability_state` 把 admission projection 写进 `CapabilityState`：`:123` 创建 capability，`:125` 插入 `next.tool.capabilities`，`:127` 扩展 `enabled_clusters`，`:132` 写 `tool_policy.include_only`。
  - `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:194` 在装配工具前调用 `execution_context_with_agent_run_admission_projection`，`:212` 直接替换 `context.turn.capability_state`。
  - `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:27` provider 用 mutated context build tools；`:67` MCP discovery 收到同一个 `context.turn.capability_state.clone()`。
  - `crates/agentdash-application-vfs/src/tools/factory.rs:46` 起各 VFS tool factory 通过 `flow.is_capability_tool_enabled` 决定是否暴露 tool；`crates/agentdash-application-vfs/src/tools/fs/platform_shell.rs:412` execution guard 也检查 `capability_state.is_capability_tool_enabled`。
  - `crates/agentdash-application-ports/src/agent_run_surface.rs:309` 定义了 `AgentRunEffectiveCapabilityPort::admit_tool`，但全库仅测试调用 `AgentRunEffectiveCapabilityService::admit_tool`，未发现产品路径调用该 admission decision port。
- Impact:
  - 工具级 grant 本应只影响执行准入，却在工具 schema 装配前扩展了 `CapabilityState`，因此可能扩大模型可见 tool surface。
  - VFS / MCP discovery / workspace module / workflow provider 看到的都是扩展后的 `CapabilityState`，无法区分“frame 可见能力”与“active grant 执行准入”。
  - 执行期 guard 继续以 `CapabilityState.is_capability_tool_enabled` 作为授权判断，AgentRun admission decision 没有成为真实执行边界。
- Contraction boundary:
  - `CapabilityResolver` / AgentFrame surface 只负责 declarative visible capability state。
  - `PermissionGrant` tool-level active grant 只进入 `AgentRunGrantProjection` / admission decision。
  - runtime tool schema exposure 消费 final visible capability view；tool invocation 消费 AgentRun admission decision，不再通过扩展 `CapabilityState` 表达准入。
- 06-14 alignment:
  - 06-14 的双事实源问题已经从 companion grant 迁移成更隐蔽的 runtime state 重写问题：`PermissionGrant` 仍是事实源，但运行时把 authorization projection 混入 capability availability state，形成事实源边界污染。

### Issue 2 - Runtime admission projection 按 run 读取 active grants，绕过 effect_frame_id 边界

- Priority: P0
- Category: authorization scope leak / frame boundary bypass
- Code evidence:
  - `crates/agentdash-domain/src/permission/entity.rs:17` 的 `PermissionGrant` 同时持有 `run_id`、`effect_frame_id`、`source_runtime_session_id`，其中 `effect_frame_id` 是授权效果锚点。
  - `crates/agentdash-domain/src/permission/repository.rs:24` 暴露 `list_by_frame(effect_frame_id, status_filter)`，`:38` 暴露 `list_active_by_frame(effect_frame_id)`；`:41` 另有 `list_active_by_run(run_id)`。
  - `crates/agentdash-application/src/permission/service.rs:24` 的 `GrantRequest` 要求传入 `effect_frame_id` 与 `source_runtime_session_id`，说明请求建模已区分效果 frame 与来源 session。
  - `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:130` 对 surface path 要求 `grant.effect_frame_id`，并以该 frame 加载/更新 capability surface；`:157` 才把 adoption target 指向 `grant.source_runtime_session_id`。
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:306` 的 runtime projection 根据 `runtime_session_id` 找 execution anchor，但 `:318` 调用 `permission_grant_repo.list_active_by_run(anchor.run_id)`，没有按 current/effect frame 收束。
  - `crates/agentdash-application/src/permission/service.rs:713` 的测试注释确认“different runtime sessions targeting same effect_frame_id are returned by list_active_by_frame”，说明 frame 查询语义存在且被视为重要边界。
- Impact:
  - 同一个 run 内不同 AgentFrame / agent / runtime session 的 active tool-level grant 可能互相污染。
  - 一个 runtime session 只要共享 `anchor.run_id`，就会把全 run active grants 编进自己的 execution capability state。
  - 在多 agent、多 frame、scope escalation 同时存在时，授权影响面从 effect frame 扩大到 run。
- Contraction boundary:
  - runtime session 应先从 execution anchor 定位当前 AgentFrame / effect frame，再用 `list_active_by_frame(effect_frame_id)` 构建 admission projection。
  - run-level grant query 只适合 UI inbox、audit、聚合列表，不应作为执行准入输入。
- 06-14 alignment:
  - 06-14 指出 `PermissionGrant` 必须成为唯一授权事实源；当前问题不是新增事实源，而是事实源读取维度错误，导致授权事实的作用域被放大。

### Issue 3 - Companion capability grant 协议仍残留旧 scope 与授权语义 UI

- Priority: P1
- Category: stale authorization projection / protocol drift
- Code evidence:
  - `crates/agentdash-application/src/companion/payload_types.rs:87` 仍注册 `capability_grant_request`，并把 `ui_hint` 设为 `capability_grant_card`；`:125` 注册 `capability_grant_result` 与 `capability_grant_result_badge`。
  - `crates/agentdash-application/src/companion/payload_types.rs:259` 的 validator 校验 `requested_paths`，但 `:283` 仍接受 `turn` / `session` / `workflow_step` scope，与当前 permission contract 的 `turn` / `agent_frame` / `activity` 不一致。
  - `crates/agentdash-application/src/companion/tools.rs:1939` 附近的 companion tool schema 仍暴露 `capability_grant_request`，scope enum 仍是 `turn` / `session` / `workflow_step`。
  - `crates/agentdash-application/src/companion/tools.rs:2006` 的 error 明确说明该路径缺少 platform permission grant broker，无法提供 `PermissionGrantService::request` policy inputs，也没有 live runtime capability update handoff。
  - `packages/app-web/src/features/session/model/companionRequestViewModel.ts:42` 仍把 `payloadType === "capability_grant_request"` 或 `uiHint === "capability_grant_card"` 映射成 capability grant view model。
- Impact:
  - UI 与 companion payload 仍表达一套旧授权请求语言，虽然 backend platform target 不实际落库，但会继续误导调用方与测试样例。
  - scope 名称与当前 permission contract 分裂，后续若接 broker 容易把旧语义重新引入 active grant 路径。
  - 它不再是 06-14 的 P0 双事实源，但仍是“授权入口尚未从 capability runtime 收束到 PermissionGrant service”的残留接口。
- Contraction boundary:
  - 删除或改造 companion `capability_grant_*` payload，使其只投影 `PermissionGrantService::request` 所需输入与当前 scope enum。
  - UI 不应从 companion payload 自造授权卡片状态；应只展示 PermissionGrant API / event 投影。
- 06-14 alignment:
  - 06-14 的 active 双事实源已被 missing broker 阻断，但 residual protocol 仍保留旧 scope 和旧 UI hint，是复发风险。

### Issue 4 - AgentRun effective/admission port 与 runtime session live port 概念分叉

- Priority: P1
- Category: interface drift / duplicated runtime contract
- Code evidence:
  - `crates/agentdash-application-ports/src/agent_run_surface.rs:309` 定义 `AgentRunEffectiveCapabilityPort`，包含 `effective_capability` 与 `admit_tool`。
  - `rg` 结果显示没有 `impl AgentRunEffectiveCapabilityPort`；`admit_tool` 的产品路径调用也未出现，只有 `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` 内的单元测试调用。
  - `crates/agentdash-application-ports/src/runtime_session_live.rs:48` 另定义 `RuntimeSessionEffectiveCapabilityPort::execution_capability_state_for_runtime_session`，返回的是 `CapabilityState`，不是 effective view 或 admission decision。
  - `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:230` 实际依赖 runtime session port 获取 mutated `CapabilityState`。
- Impact:
  - 规范里要求的“tool schema exposure 消费有效 capability view；tool execution 消费 admission decision”在接口层有形态，但运行时产品路径走的是另一套 `CapabilityState` mutation 口。
  - 后续新增 MCP / VFS / workspace module provider 时，很容易继续接入 `CapabilityState.is_capability_tool_enabled`，扩大 Issue 1。
- Contraction boundary:
  - 保留一个 AgentRun runtime boundary：effective view 用于 schema exposure，admission decision 用于 execution。
  - `RuntimeSessionEffectiveCapabilityPort` 若继续存在，应返回 view + projection 或 decision service，而不是返回被 grant projection 改写过的 `CapabilityState`。
- 06-14 alignment:
  - 这是 06-14 “runtime provider 过宽 / capability 与 tool catalog 分散”的后续形态；composer 已拆分，但最终授权边界仍没有统一到 AgentRun admission。

### Code patterns

- `CapabilityResolver` 的 declarative baseline 仍清晰：`crates/agentdash-application/src/capability/resolver.rs:275` merge contributions，`:278` 计算 `default_visible_capabilities`，`:283` reduce directives，`:339` 从 effective caps 编译 ToolCluster / MCP scope，`:361` 编译 tool policy，`:374` 返回 `CapabilityState`。
- `default_visible_capabilities` 只扫描 `WELL_KNOWN_KEYS` 并按 owner / agent declaration / workflow active 判断可见性：`crates/agentdash-application/src/capability/resolver.rs:434`。
- 自定义 MCP 是 open key，但 runtime server 注入需要 preset 命中：`crates/agentdash-application/src/capability/resolver.rs:302` 检查 `cap.is_custom_mcp()`，`:305` 从 `mcp_candidates.presets` 取 preset，`:316` 对 missing preset 只发 warning。
- tool catalog 当前由后端 SPI catalog 投影：`crates/agentdash-application/src/capability/tool_catalog.rs:79` 查询 known key tools，`:86` 对 `mcp:<server>` 返回 runtime-discovered placeholder，`:105` 查询 capability catalog，`:132` 从 `default_visibility_rules()` 投影 allowed scope / grantability。
- Contract 作为投影证据成立：permission 与 workflow catalog 的 Rust contract 源在 `crates/agentdash-contracts`，前端 `generated/*-contracts.ts` 消费这些 DTO；未发现 06-14 提到的前端手写 catalog 常量残留。

### Related specs

- `.trellis/spec/backend/capability/architecture.md`：CapabilityResolver 是 session declarative tool baseline；tool schema exposure 应消费 AgentRun final visible capability view；tool execution 应消费 AgentRun admission decision。
- `.trellis/spec/backend/permission/architecture.md`：`PermissionGrant` 是授权事实源；policy 是 pure function；tool-level grant 进入 AgentRun admission projection；surface-changing grant 写 AgentFrame revision；runtime adoption 失败必须可见。
- `.trellis/spec/backend/vfs/architecture.md`：runtime tool surface 由 `SessionRuntimeToolComposer` 与 domain providers 组合；provider 以 capability state gate tool exposure。
- `.trellis/spec/cross-layer/architecture.md`：Contract 是跨层投影源，不应由前端手写镜像业务目录。

### External references

- None. 本次审查仅使用仓库内任务文档、Trellis spec、业务代码与 06-14 baseline。

## Caveats / Not Found

- 未跑全量测试，符合用户约束；本文件是静态架构审查，不声明运行时复现结果。
- 未修改业务代码、spec、task 文档或其他 research 文件。
- 未发现当前前端仍存在 06-14 点名的 `CAP_EDITOR_WELL_KNOWN_KEYS` / `AUTO_GRANTED_BASELINE` 旧常量残留。
- Contract 仅作为投影证据使用；没有把 Contract 单独作为被审查模块。
- 本轮未展开数据库 migration 审查，因为范围集中在授权事实源、capability runtime state 与 tool exposure/admission 边界。
