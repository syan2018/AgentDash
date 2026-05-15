# Current To Target Migration

## Target Boundary

目标不是删除一个旧类型，而是恢复四段边界：

```text
Source Adapter -> LaunchCommand -> SessionConstructionPlan -> LaunchExecution -> ExecutionContext
```

- `LaunchCommand`：只表达来源意图和引用。
- `SessionConstructionPlan`：session 构建事实源，统一 owner / workspace / VFS / MCP / capability / context / identity / projections / trace。
- `LaunchExecution`：一次 launch 的短生命周期执行计划，统一 prompt / lifecycle / restore / hook / follow-up / runtime command / terminal effect / connector input / trace。
- `ExecutionContext`：connector SPI 投影，不是 application 主链路事实源。

## Current To Target Map

| Boundary | Current State | Required Move |
|---|---|---|
| `LaunchCommand` | 已是生产入口；不再持有 `PromptAugmentInput`；`to_augment_input()` 已删除；local relay 不再携带已组装 `Vfs`；task handler、companion snapshot、working_dir、continuation context frame 已移出 command；local relay MCP 已收窄为 declaration source payload；source contract、source identity、local relay root/MCP declarations 不再写入 construction facts；`SessionLaunchPlanner` 现在直接从 command 投影 source contract、identity、follow-up、local relay payload | 继续保持 source payload 只能从 command 进入 planner/construction，不允许回流到 facts handoff |
| `UserPromptInput` | 已移除 `working_dir`；prompt input 只保留 prompt/env/executor override；`SessionConstructionFacts` 也不再携带 working dir hint / source identity / source contract / local relay source payload | 保持这些 source-side facts 归零；working directory 只能从 VFS default mount / local relay workspace root / workspace 事实解析进 construction |
| Source adapters | 多数入口已构造 command；task handler 与 companion parent VFS/MCP/context snapshot 已移出 command；local relay workspace root/MCP declarations 已作为 source fact 进入 planner/construction 解析；task effect binding 已改为 durable `TerminalHookEffectBinding` | adapters 只能交出请求意图、来源引用和特殊来源策略 payload；bootstrap 上的 companion 临时投影需迁入 construction provider，task binding 生成也需继续从 API bootstrap 下沉 |
| `PromptAugmentInput` | 已删除，不再跨 API/bootstrap/application 传递 | 保持归零 |
| `SessionLaunchRequest` | 已删除，不再作为生产 handoff | 保持归零；剩余 `SessionConstructionFacts` 不能扩张为新边界，需继续拆入 construction/launch/effects |
| `SessionConstructionPlan` | 已有类型；context plan 已保留完整 bundle；source identity 已由 command 显式投影进入 plan | 补齐 VFS、MCP、capability、executor、task effect binding、companion slice、audit/inspector projection |
| Context endpoint | route 层大部分重建已迁走 | query/audit/inspector 与 launch 投影同一 construction |
| `SessionLaunchPlanner` | 已不直接消费旧 payload；source intent 已由 `LaunchCommand` 原件进入 planner | 消费 `LaunchCommand + SessionConstructionPlan + runtime facts`，删除 `SessionConstructionFacts` provider handoff |
| `prompt_pipeline` | 不再重组 source contract / identity / relay source facts；仍接收 provider facts 并参与 execution setup | 只执行 `LaunchExecution`，不再承接 construction facts handoff |
| `SessionHub` | 仍是能力聚合入口 | 拆成 core / ownership / construction / launch / runtime / eventing / hooks / effects / pending / adapters |
| Effects / Pending | outbox 与 runtime command store 已有基础 | 补 durable identity、apply-once、failed/retry/recovery、migration 验证 |

## Migration Steps

### Step 1: Correct Entry Intent

- Keep `working_dir` / working dir hint out of `UserPromptInput` and `SessionConstructionFacts`.
- Keep `LaunchCommand` limited to source, actor, target ids, prompt, executor override, follow-up hint, source policy payload.
- Keep source contract、source identity、local relay workspace root、local relay MCP declarations out of `SessionConstructionFacts`; planner/construction 只能从 `LaunchCommand` 原件投影这些 source facts.
- Keep task `post_turn_handler` out of `LaunchCommand`; keep task effects as durable construction/effects binding and move binding generation out of API bootstrap.
- Keep companion parent VFS/MCP/context snapshots out of `LaunchCommand`; move the current bootstrap parent capability projection into construction provider.
- Keep local relay workspace root as source fact and let construction/launch resolve VFS; keep MCP only as declaration and never as facts-resolved MCP.

### Step 2: Complete Construction

- Resolve working dir from owner/workspace/agent/lifecycle/local relay root in construction.
- Resolve VFS, MCP declarations, capability state, executor profile in construction; source identity projection must stay command-derived.
- Resolve companion slice through construction providers, and move task effect binding generation into the same construction provider layer.
- Add context frame plan, audit projection, inspector projection.

### Step 3: Collapse Launch

- Build `LaunchExecution` from `LaunchCommand + SessionConstructionPlan + runtime facts`.
- Project connector input from `LaunchExecution`.
- Remove request/meta/profile fallback from `prompt_pipeline`.

### Step 4: Delete Old Payload And Envelope

- Keep `PromptAugmentInput` deleted.
- API/bootstrap returns construction facts or construction plan input, not a generalized launch envelope.
- Keep `SessionLaunchRequest` deleted.
- Remove `SessionConstructionFacts` from production mainline after construction/effects/launch fields have owners.

### Step 5: Split Hub And Verify Stores

- Move business methods out of `SessionHub`.
- Verify terminal effects, pending runtime command, and persistence store boundaries.

## Forbidden Final-State Explanations

- `PromptAugmentInput` as production payload.
- `SessionLaunchRequest` as final production boundary.
- `SessionConstructionFacts` as final production boundary.
- `LaunchCommand` carrying resolved VFS/MCP/context/capability/hook/effect/working_dir.
- `UserPromptInput.working_dir`.
- route/bootstrap rebuilding owner/context/VFS/capability outside construction.
- `prompt_pipeline` planning or fallback.
- in-memory callback as terminal effect source of truth.
- `SessionHub` as business facade.
- `SessionPersistence` as a catch-all store for new business logic.
