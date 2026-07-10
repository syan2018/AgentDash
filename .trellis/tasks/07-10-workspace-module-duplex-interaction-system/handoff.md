# Handoff · Workspace Module 通用双工交互系统

## 1. 当前状态

- Trellis task：`workspace-module-duplex-interaction-system`
- 状态：`planning`，尚未执行 `task.py start`。
- 产品决策已经收敛；本轮只更新任务规划产物，没有修改生产代码、数据库或 generated contracts。
- 现状审计覆盖 Operation/RuntimeGateway、Rhai runtime、Canvas/Extension UI、Interaction、Channel 边界、旧 Canvas tables/routes 和历史 Channel 任务。
- 父任务内部通过 `work-items/` 统一管理落实步骤、依赖、状态和验收证据。

优先阅读顺序：

1. [prd.md](./prd.md)
2. [design.md](./design.md)
3. [implement.md](./implement.md)
4. [research/freeze-review.md](./research/freeze-review.md)
5. [work-items/decisions.md](./work-items/decisions.md) 与 [work-items/README.md](./work-items/README.md)
6. [research/current-state.md](./research/current-state.md)
7. 相关 Channel 任务：[../07-10-channel-domain-boundary-refactor-evaluation/design.md](../07-10-channel-domain-boundary-refactor-evaluation/design.md)

## 2. 已确认的目标模型

### OperationScript

- OperationScript 是无独立持久化的一次性执行请求：`language=rhai_v1 + host_api_version + source + input + allowed_operations + limits`。
- Application executor/host-call port 是 async；Rhai V1 语法同步，execution-scoped `ops.invoke` 隐式等待，`ops.invoke_all` 提供 bounded structured concurrency。
- Infrastructure 复用现有 Rhai limits/JSON bridge/AST cache 设计，通过 evaluator factory + bounded worker pool + async request/response bridge 执行；纯 CPU loop 与 nested Operation 均响应 cancellation/deadline。
- Agent、Canvas/UserWorkshop 与 Workflow 调用同一 executor。Canvas 可保存 `.rhai` 或 inline source，但完整脚本交给服务端执行，iframe 不解释脚本。
- Rhai 只获得受控 `invoke` / `invoke_all` host functions。每个 nested Operation 都重新进入 canonical execution core，重新校验 provider、schema、capability、effect、readiness 和 limits。
- preflight token 绑定 dialect/host API、source/input/manifest/limits/principal/scope/expiry；execution 受 worker/caller cancellation、timeout、调用数、并行度和输出上限约束，并禁止递归脚本。
- 不建设 script asset、background job、durable execution aggregate、跨调用 REPL state、Workflow retry/gate/recovery 或隐式 LLM 调用。

### InteractionInstance

- Personal definition 产生 User-owned instance；Project definition 产生 Project-owned instance。AgentRun 只作为可选 attachment。
- tab、renderer、AgentRun 或历史 RuntimeSession 结束不删除 instance；explicit close + retention 管理生命周期。
- Human 与 Agent 使用同一 typed command；policy 只有 `direct` 和 `human_only`。后者拒绝 Agent canonical write，Agent 可经 Channel 发送非权威 suggestion，由 Human 重新提交命令。
- canonical state transition 由平台 Interaction service 确定性执行，只提供有限通用 state commands 与少量 typed handlers；没有 generic reducer registry、DSL、Extension reducer 或 proposal aggregate。
- V1 generic mutation 固定为 bounded `state_patch_v1`；JSON Pointer allowlist、schema、expected revision、event/state 在单事务校验与提交。
- reliable command effect 只通过 replay-safe 单 OperationEffectIntent outbox；OperationScript 不自动 replay，复杂 durable multi-step effect 归 Workflow。
- InteractionDefinition 使用 immutable revision + optimistic CAS；草稿留在客户端，不建设 durable draft/CRDT。
- instance 固定 definition revision 和 exact Extension artifact digest；升级只影响新 definition/new instance，旧 artifact 不可用时返回 structured unavailable，不建设通用 state migration engine。

### Extension / Canvas

- Extension 贡献 Component + canonical Operation。Component 接收 props/state projection、发出 typed event，由 Interaction definition binding 映射到平台 command 或 OperationScript。
- 第三方 component 使用 isolated iframe + scoped MessageChannel；组件不获得 Project/session/backend/workspace authority 或通用 invoke 权限。
- 现有 runtime actions、protocol methods 与 backend services 统一复用 canonical Operation；Extension 自有复杂状态保留在其 service/Operation。
- 项目没有需要保留的 Canvas 存量。旧 Canvas aggregate 和 runtime snapshot 模型一次性由最终 Interaction 模型替换，不 backfill、不双读、不提供旧 decoder。
- Canvas source 使用 immutable SourceBundle + VFS changeset CAS；Personal publish、Project unpublish、copy-to-personal、resource binding 与 Extension promotion 固定 exact revision lineage。
- authoring 与 shared runtime identity 分离为 `canvas:{definition_id}` / `canvas://{definition_id}` 和 `interaction:{instance_id}` / `interaction://{instance_id}`。
- V1 definition/interaction/script/component/Operation contracts 均带显式 version；future breaking change 使用 V2 + 显式 migration，不改变既有 instance 语义。

### RuntimeGateway / Channel

- RuntimeSession 不提供 RuntimeGateway authority、scope、surface 或 placement，并最终从 Canvas/Extension contracts 删除。
- Canvas/Extension 是 invocation origin，不是 security principal；principal/scope/origin/placement/trace 在内部 envelope 中正交表达。
- browser 不提交 session id、backend id、workspace root 或预组装 capability；每次 invoke 重新 admission。
- Channel 只负责 stateful communication、attention、handoff 和 delivery，不拥有 Interaction command/event/state。Extension provider API 改称 ExtensionProtocol，并投影为 Operation。

## 3. 核心实施顺序

1. WI-00：把已确认合同写入权威 specs，并形成旧路径删除矩阵。
2. WI-01：canonical Operation、RuntimeInvocationEnvelope 与唯一 execution core。
3. WI-02：UserWorkshop adapter、Canvas standalone bridge 和失效 endpoint 清理。
4. WI-03：OperationScript async executor / Rhai evaluator factory / Agent、Canvas、Workflow callers。
5. WI-04：V1 definition/SourceBundle/instance、`state_patch_v1`、EffectIntent 和 attachments。
6. WI-05：Canvas distribution/VFS/resource/promotion 最终替换和旧五张表、routes、DTO、repositories 清理。
7. WI-06：Extension component ABI、isolated host、exact artifact binding。
8. WI-07/WI-08：Workspace Module projection 收束与 Workflow/Channel 端到端集成。
9. WI-09：RuntimeSession removal sweep。
10. WI-10：全量代码、contract、frontend/browser、migration、spec 和残留验证。

详细依赖和退出条件见 `work-items/README.md` 与各 `WI-*.md`。

## 4. 当前已知代码事实

- Canvas 当前在浏览器内把 TS/JS 编译为 Blob module，通过普通 JavaScript 组合单次 invocation；Agent 没有 headless script execution 入口。
- shared Infrastructure 已有 bounded Rhai runtime、AST cache、JSON bridge 和 operation/call-depth/string/array/map limits，可作为首个 OperationScript adapter 基础。
- 当前 Rhai evaluator/host function 是同步 API，不能直接承载 async Gateway；目标使用 execution-scoped evaluator factory 和 bounded worker/bridge，而不是在 Tokio core worker 上 `block_on`。
- 后端已删除 `/canvases/{id}/runtime-snapshot`，普通 Project Canvas preview 仍调用它；WI-02 在建立 standalone bridge 后删除该断口。
- 旧 schema 包含 `canvases`、`canvas_files`、`canvas_bindings`、`agent_run_canvas_runtime_observations`、`agent_run_canvas_interaction_snapshots`；WI-05 用新 migration 直接删除。
- 当前 RuntimeGateway actor/context 仍依赖 Session；当前 hook auto-resume 还会用 `effect_id` 伪造 channel id，Channel broadcast service admission 也未闭合。

## 5. 下一步

冻结审查没有发现需要改变主架构的产品分支；V1 version/source/state/effect/distribution/resource contracts 已补入规划。下一步是用户最终批准；批准后才执行 `task.py start`，从 WI-00 开始进入实现。

实施中需要从代码推导的事项包括 Rust 类型/package ownership、Rhai host bridge 细节、Interaction schema/index、artifact retention repository 和精确删除 write set。只有这些证据会改变产品语义时，才重新打开规划讨论。

## 6. 交付基线

- 初始 planning artifacts 位于 commit `38f942c2`；后续状态以 `git status`、`git log` 和 task tracker 为准。
- `AGENTS.md` 没有本任务改动。
- 规划阶段校验 task JSON、JSONL context、work-item links、术语残留与 `git diff --check`；没有生产代码变更时不运行代码测试。
