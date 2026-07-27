# WI-11 Workspace Module Agent 能力最终收口

Status: completed

Depends On: WI-03、WI-07、PR #95 最终 Runtime 接线

## Problem

PR #95 合入后，Workspace Module 的主体方向已经是 projection-only，但原始 V1 合同没有在生产
接线和 Agent 使用面完整闭环：

- Workspace Module 已只负责 Extension/Interaction/Operation descriptor projection；该职责属于
  application actor projection，不再形成独立 crate 边界。
- 生产 `workspace_module_present` 要求 Agent 提交完整 `WorkspaceModulePresentation`，而 Skill
  仍声明只提交 `module_id + view_key`；renderer、URI 和 title 因而由不可信调用方重复构造。
- 旧的服务端 descriptor lookup、Canvas Interaction attachment 和 canonical presentation 构造仍留在
  未挂载的 `bootstrap/operation_runtime_tools.rs`，不参与编译或生产接线。
- OperationScript engine、UserWorkshop API 和 Workflow caller 已存在，但 Agent runtime 没有
  preflight/run 入口；Agent 只能连续执行多个单次 `workspace_module_invoke`，无法使用原始需求中的
  有界组合、筛选、聚合、structured concurrency 和 call evidence。
- `InteractionDefinitionRevision` 尚未实现原设计中的显式 `agent_projection`，无法以 definition
  contract 限定 Agent 可观察的 canonical state；当前 Agent-facing Operation 只覆盖 command。
- Component event binding 目前只能映射到 platform command，未实现原设计允许的即时 Operation 或
  OperationScript action；Canvas runtime 也没有把 definition 中保存的 `.rhai`/inline source 接入
  UserWorkshop OperationScript host。
- Workspace Module 只生成 `canvas:{definition_id}` 与 `ext:{extension_key}`，未形成
  `interaction:{instance_id}` shared runtime module/attachment discovery；Interaction command Operation
  虽要求 `instance_id`，Agent 却没有同源的 instance runtime surface。
- `workspace-module-system` Skill 没有说明 Extension runtime action/protocol/backend service 最终统一为
  canonical Operation，也没有说明 visibility/readiness/effect/replay/provenance、组合方式和 stale
  actor surface。

## Target Contract

### 1. Boundary

- Workspace Module 只投影 actor-visible module、UI entry 和 canonical Operation descriptor。
- OperationGateway 独占 Operation discovery、authority、readiness、placement、dispatch、result 与 audit。
- Interaction service 独占 definition、instance、attachment 与 canonical state。
- Interaction definition 独占 Agent state projection contract；Workspace Module 只消费 projection，
  不直接读取或筛选任意 instance state。
- OperationScript engine 独占一次性多 Operation 组合；Workflow 独占 durable multi-step orchestration。
- Extension `operation_catalog` 只把一个 Agent-visible Operation 映射到一个 runtime action、protocol
  method 或 backend service，不把该映射描述为组合执行。

### 2. Agent discovery and single invoke

```text
workspace_module_list
  -> workspace_module_describe(module_id)
  -> exact OperationRef + schema/readiness/effect/replay/provenance
  -> workspace_module_invoke(operation_ref, input)
  -> current actor surface revalidation
  -> OperationGateway::invoke
```

- list/describe/invoke 每次从 RuntimeThread Product binding、immutable AgentFrame revision、applied
  resource surface、WorkspaceModuleDimension 和 current OperationGateway surface解析事实。
- invoke 保持单 Operation 语义，不接受任意 operation 数组或 inline script。

### 3. Agent OperationScript composition

- 暴露 Agent-facing `operation_script_preflight` 与 `operation_script_run`，复用 WI-03 的
  `OperationScriptEngine`、preflight token、limits、result ref 和 Gateway nested executor。
- Agent 从一个或多个 `workspace_module_describe` 结果选择完整 OperationRefs；服务端必须重新从当前
  actor surface 解析 descriptor、digest、effect、replay policy、authority revision 与 granted
  capabilities，不能信任 Agent 提交的 manifest 元数据。
- preflight token继续绑定 dialect/host API、source、input、allowed descriptors、limits、
  principal/scope 和 expiry；run 与每个 nested call 都重新 admission。
- 即时组合失败返回 bounded diagnostic、partial/outcome-unknown 与 call evidence；需要 durable
  retry、human gate、recovery 或跨会话状态的编排必须进入 Workflow。

### 4. Interaction Agent projection and runtime module

- `InteractionDefinitionRevision` 增加显式、版本化的 Agent state projection contract。V1 只允许
  allowlisted JSON Pointer/path projection，不允许 Agent 默认读取完整 state。
- Interaction application service 根据 pinned definition revision 与 current instance state 生成
  Agent projection；projection 结果携带 instance/state revision，不能成为写入 authority。
- Workspace Module 同时区分：
  - `canvas:{definition_id}` / `canvas://{definition_id}`：authoring definition 与 preview；
  - `interaction:{instance_id}` / `interaction://{instance_id}`：已授权 attachment 对应的 shared
    runtime 与 canonical state projection。
- AgentRun surface 只投影当前 actor 已授权的 Interaction attachments。`interaction:*` describe 返回
  pinned definition/revision 对应的 typed command Operations，以及只读 Agent state projection；
  invocation 仍要求 exact OperationRef、instance id、command id 与 expected state revision。
- definition、instance、attachment、renderer lease 与 Workspace tab identity 保持不同生命周期；
  presentation 不通过 URI 或历史 frame 猜测 attachment。

### 5. Component and Canvas actions

- Component event binding 使用显式 tagged target：
  - versioned platform command；
  - exact single Operation；
  - ephemeral OperationScript program/binding。
- 三种 target 都只做 schema validation 与 payload pass-through，不增加 expression mapping 或 reducer
  DSL；Operation/OperationScript 通过 UserWorkshop/Interaction host 重新建立 principal、scope、
  authority 和 allowed Operation manifest。
- Canvas definition 可以把 `.rhai` 文件或 inline source 保存为 immutable SourceBundle 内容，并由
  Canvas host/component event 触发完整 OperationScript request；iframe 不执行 Rhai，不获得
  credentials、backend id、placement 或 bearer authority。
- OperationScript action 是即时结果，不自动修改 Interaction state、不进入 durable effect outbox；
  需要可靠状态提交时使用 command + replay-safe single OperationEffectIntent，需要 durable 多步执行时
  进入 Workflow。

### 6. Trusted presentation

Agent contract：

```text
workspace_module_present(module_id, view_key, payload?)
```

服务端：

```text
resolve current module surface
  -> validate module/view visibility
  -> derive renderer_kind/presentation_uri/title
  -> Canvas definition: create or reuse canonical Interaction attachment
  -> emit typed WorkspaceModulePresentation
```

- Agent 不提交 `renderer_kind`、`presentation_uri`、`title` 或 attachment/diagnostics authority。
- 前端继续对 live presentation 与刷新后的 current Workspace Module descriptor 做精确一致性校验。
- HTTP 用户主动打开模块与 Agent live presentation 复用同一 canonical presentation builder，但分别
  使用 UserWorkshop 与 AgentRun authority。

### 7. Skill

`workspace-module-system` 必须至少说明：

- Workspace Module 是 projection，不是 execution/state authority。
- `canvas:{definition_id}`、`canvas://{definition_id}`、`interaction://{instance_id}`、
  `ext:{extension_key}` 的区别。
- list/describe/invoke/present 的准确参数和推荐顺序。
- panel-only、agent-and-panel、readiness、effect、replay policy、schema、permission、provenance 的含义。
- 单次 invoke、OperationScript 即时组合和 Workflow durable orchestration 的选择边界。
- allowed OperationRefs 必须来自 describe，但 preflight/run 仍以服务端 current surface 为准。
- surface 变化后重新 describe，不缓存或重建 exact ref/presentation。

## Write Set

- `crates/agentdash-application/src/extension_runtime.rs`
- `crates/agentdash-application/src/workspace_module.rs`
- `crates/agentdash-domain/src/workspace_module/skills/workspace-module-system/`
- `crates/agentdash-application/src/runtime_tools/`
- `crates/agentdash-application-ports/src/operation_script.rs`
- `crates/agentdash-infrastructure/src/runtime_tool_executors.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/bootstrap/operation_runtime_tools.rs`
- `crates/agentdash-api/src/routes/workspace_module.rs`
- `crates/agentdash-domain/src/interaction/`
- `crates/agentdash-application/src/interaction/`
- `crates/agentdash-api/src/routes/interactions.rs`
- `crates/agentdash-contracts/src/surface/interaction.rs`
- `packages/app-web/src/features/canvas-panel/`
- `packages/app-web/src/features/extension-runtime/`
- Agent runtime contract / protocol projection 与 generated TypeScript（按实际影响）
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Exit Criteria

- [x] Workspace Module projection 已收回 `agentdash-application`，不再保留无独立
      authority、port 或 domain contract 的 crate。
- [x] 未挂载的旧 Workspace Module runtime tool provider 被删除；仍有效的可信 presentation 行为迁入
      唯一生产路径。
- [x] list/describe/invoke/present 的 schema、Skill 与实际生产工具一致。
- [x] Agent 无法提交 renderer、URI、title、attachment 或 diagnostics authority。
- [x] Agent 可以从 Workspace Module descriptors 选择至少两个 exact Operations，通过同一个
      OperationScript preflight/run 完成组合、结果处理和 call evidence 返回。
- [x] `InteractionDefinitionRevision` 具备显式 V1 Agent projection contract；Agent 只能观察
      allowlisted state，projection 携带 instance/state revision 且不能充当写入授权。
- [x] AgentRun 可以发现当前已授权 attachment 对应的 `interaction:{instance_id}` runtime module，
      describe 获得 pinned command Operations 与 Agent state projection，并能按 expected revision
      提交 direct command；human-only command 不进入 Agent operation surface。
- [x] Component event 可以按 tagged contract 触发 platform command、single Operation 或
      OperationScript；Canvas 保存的 `.rhai`/inline source 可经 host 调用同一 UserWorkshop
      OperationScript engine，iframe 不执行脚本或持有 authority。
- [x] capability/readiness 撤销、descriptor/token drift、schema 错误、timeout/cancel 和 partial
      side effect 均在正确层失败。
- [x] Extension runtime action、protocol method、backend service 均只通过 canonical Operation 暴露；
      没有新增第二套 dispatch 或编排事实源。
- [x] 即时 OperationScript 与 durable Workflow 的边界进入 Skill 和相关 specs。
- [x] Rust contracts、generated TypeScript、frontend live presentation consumer 与 specs 同步。

## Validation

- Workspace Module projection、Product runtime tool 和 Native Agent tool projection focused tests。
- OperationScript Agent caller 的 preflight token、双 Operation、parallel limit、cancel/timeout、
  descriptor drift、partial evidence tests。
- Canvas definition presentation、Interaction attachment、Extension webview presentation 与 stale
  descriptor rejection tests。
- Interaction Agent projection path/schema/secret isolation、attachment visibility、state revision 与
  `interaction:*` module currentness tests。
- Component/Canvas command、single Operation、OperationScript 三类 event target 的 schema、authority、
  cancellation 与“不自动写 state/outbox”测试。
- `cargo check` / `cargo test` 仅覆盖受影响 crates；Cargo workspace lock 被 IDE 占用时先观察进程。
- `pnpm run contracts:check`
- 受影响 frontend TypeScript / focused tests。
- `pnpm run migration:guard`（仅当 schema/migration 实际变化）。
- `rg` 静态扫描旧 provider、重复 presentation builder、旧参数 schema 与过时 Skill 示例。
- `git diff --check`

## Progress

- 2026-07-27：从 `origin/codex/workspace-duplex-interaction-planning` 恢复父任务到非归档区。
- 2026-07-27：基于 PR #95 合入后的生产代码复核建立本 WI。
- 2026-07-27：补齐 Agent `operation_script_preflight/run`、可信 presentation、显式 Agent state
  projection、attachment-scoped `interaction:*` runtime module、component tagged target 与
  SourceBundle `.rhai`/inline host execution。
- 2026-07-27：删除未挂载旧 provider，更新 embedded Skill、Trellis specs、generated TypeScript 与
  Canvas frontend event path。
- 2026-07-27：通过受影响 Rust crates check/test target、Workspace Module/Agent projection/tool
  focused tests、app-web typecheck、contracts check、migration guard 与 `git diff --check`。
- 2026-07-27：将 projection-only Workspace Module 收回 `agentdash-application` 并删除独立 crate；
  当前数据库已重建，因此 schema 直接使用最新 document contract，不保留仅更新旧数据的 0002。
- 2026-07-27：按 Skill 渐进披露合同重写 `workspace-module-system`：description 同时覆盖能力与
  Canvas/Interaction/Extension、单次调用、UI presentation 和即时组合触发场景；正文收束为
  list/describe/invoke/present/OperationScript 的可执行决策与 exact 参数示例。
